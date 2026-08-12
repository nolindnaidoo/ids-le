//! Key paths in YAML block style: indentation is the structure.
//!
//! **A line scanner, not a parser.** It reads block mappings and block
//! sequences, which is what a configuration file is written in. Flow
//! style (`{a: 1}`), anchors, aliases, multi-line scalars and multiple
//! documents in one file are not modelled — a value inside one of those
//! carries the key path of the line it sits on, or none.
//!
//! The rule that makes indentation work here is that a key is pushed at
//! the **column its own text starts at**, not at the line's indent. In
//!
//! ```yaml
//! items:
//!   - id: A
//!     name: B
//! ```
//!
//! `id` starts at column 4 because `- ` consumed two, so `name` at
//! column 4 replaces it rather than nesting under it — which is exactly
//! what the document means. Pushing at the line indent would name it
//! `items.[0].id.name`.

use super::locate::{KeySpan, join, lines, value_length};

/// YAML has one comment character.
const COMMENT: &[u8] = b"#";

pub(crate) fn key_spans(text: &str) -> Vec<KeySpan> {
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut counters: Vec<(usize, usize)> = Vec::new();
    let mut spans = Vec::new();

    for (offset, line) in lines(text) {
        let content = line.trim_start();
        if content.is_empty() || content.starts_with('#') || content.starts_with("---") {
            continue;
        }
        let indent = line.len() - content.len();
        stack.retain(|(at, _)| *at < indent);
        counters.retain(|(at, _)| *at <= indent);

        let mut column = indent;
        let mut body = content;
        if let Some(rest) = body.strip_prefix('-') {
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            let index = next_index(&mut counters, indent);
            stack.push((indent, format!("[{index}]")));
            column += body.len() - rest.len();
            body = rest;
        }

        let Some((key, value_at)) = split_key(body) else {
            // A sequence item holding a scalar rather than a mapping.
            let length = value_length(body, COMMENT);
            if length > 0 {
                spans.push(KeySpan {
                    start: offset + column,
                    end: offset + column + length,
                    path: path_of(&stack),
                });
            }
            continue;
        };
        stack.push((column, key.to_string()));

        let value = &body[value_at..];
        let leading = value.len() - value.trim_start().len();
        let length = value_length(value, COMMENT);
        // A key introducing a nested block, and a key whose line holds
        // nothing but a comment, are the same thing here: no value. The
        // comment case has to be caught rather than clamped — the span
        // would otherwise end before it started.
        if length <= leading {
            continue;
        }
        spans.push(KeySpan {
            start: offset + column + value_at + leading,
            end: offset + column + value_at + length,
            path: path_of(&stack),
        });
    }
    spans
}

/// The next index for a sequence at one indent, remembered so a second
/// `- ` on the following line is `[1]` rather than `[0]` again.
fn next_index(counters: &mut Vec<(usize, usize)>, indent: usize) -> usize {
    if let Some((_, next)) = counters.iter_mut().find(|(at, _)| *at == indent) {
        *next += 1;
        return *next - 1;
    }
    counters.push((indent, 1));
    0
}

/// A key and where its value starts, for `key: value`.
///
/// The colon has to be followed by a space or end the line, which is
/// what keeps `url: https://example.com` from being split at the second
/// colon and named `url: https`.
fn split_key(body: &str) -> Option<(&str, usize)> {
    let bytes = body.as_bytes();
    for (at, byte) in bytes.iter().enumerate() {
        if *byte != b':' {
            continue;
        }
        if at + 1 == bytes.len() || bytes[at + 1] == b' ' {
            let key = body[..at].trim().trim_matches(['"', '\'']);
            return (!key.is_empty()).then_some((key, at + 1));
        }
    }
    None
}

fn path_of(stack: &[(usize, String)]) -> String {
    join(stack.iter().map(|(_, segment)| segment.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyed(text: &str, expected: &[(&str, &str)]) {
        let spans = key_spans(text);
        let found: Vec<(&str, &str)> = spans
            .iter()
            .map(|span| (text[span.start..span.end].trim(), span.path.as_str()))
            .collect();
        assert_eq!(found, expected, "{text}");
    }

    #[test]
    fn a_value_is_named_by_its_key() {
        keyed("id: abc\n", &[("abc", "id")]);
    }

    #[test]
    fn indentation_builds_a_dotted_path() {
        keyed("a:\n  b:\n    c: x\n", &[("x", "a.b.c")]);
    }

    #[test]
    fn a_sibling_replaces_rather_than_nests() {
        keyed("a:\n  b: x\n  c: y\n", &[("x", "a.b"), ("y", "a.c")]);
    }

    #[test]
    fn dedenting_pops_back_to_the_outer_key() {
        keyed("a:\n  b: x\nc: y\n", &[("x", "a.b"), ("y", "c")]);
    }

    #[test]
    fn a_sequence_of_scalars_is_indexed() {
        keyed(
            "ids:\n  - x\n  - y\n",
            &[("x", "ids.[0]"), ("y", "ids.[1]")],
        );
    }

    /// The reason a key is pushed at its own column. Pushing at the line
    /// indent would name the second field `rows.[0].id.name`.
    #[test]
    fn a_sequence_of_mappings_keeps_the_index_and_each_key() {
        keyed(
            "rows:\n  - id: x\n    name: y\n  - id: z\n",
            &[
                ("x", "rows.[0].id"),
                ("y", "rows.[0].name"),
                ("z", "rows.[1].id"),
            ],
        );
    }

    #[test]
    fn a_key_introducing_a_block_is_not_itself_a_value() {
        assert_eq!(key_spans("a:\n  b: x\n").len(), 1);
    }

    #[test]
    fn comments_and_document_markers_are_skipped() {
        keyed("---\n# id: 1\nid: 2\n", &[("2", "id")]);
    }

    /// The second colon must not become the separator.
    #[test]
    fn a_colon_inside_a_value_does_not_split_the_key() {
        keyed(
            "url: https://example.com\n",
            &[("https://example.com", "url")],
        );
    }

    #[test]
    fn a_quoted_key_loses_its_quotes() {
        keyed("\"my key\": x\n", &[("x", "my key")]);
    }

    /// **A comment is not part of the value.** The key path is evidence,
    /// so a trailing comment holding a run must not borrow the line's
    /// key — which is how `_id: 1 # <24 hex>` named a hash an ObjectId.
    #[test]
    fn a_trailing_comment_is_outside_the_value() {
        keyed("id: 1 # 6a7bb780a1b2c3d4e5f60718\n", &[("1", "id")]);
    }

    /// A `#` with no space in front of it is part of a plain scalar,
    /// which is what YAML says and the conservative answer besides.
    #[test]
    fn a_hash_inside_a_scalar_belongs_to_the_value() {
        keyed("id: a#b\n", &[("a#b", "id")]);
        keyed("id: \"a # b\"\n", &[("\"a # b\"", "id")]);
    }

    /// A line whose value is nothing but a comment introduces a block —
    /// it does not carry a value, and it must not carry a span running
    /// backwards from one.
    #[test]
    fn a_key_whose_value_is_only_a_comment_carries_nothing() {
        assert!(key_spans("id: # 6a7bb780a1b2c3d4e5f60718\n").is_empty());
        assert_eq!(key_spans("a: # nothing\n  b: x\n").len(), 1);
    }

    #[test]
    fn the_spans_are_ordered_and_do_not_overlap() {
        let spans = key_spans("a: 1\nb:\n  - 2\n  - 3\nc: 4\n");
        assert_eq!(spans.len(), 4);
        for pair in spans.windows(2) {
            assert!(pair[0].end <= pair[1].start, "{pair:?}");
        }
    }
}
