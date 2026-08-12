//! Key paths in TOML: a table header, and one key per line.
//!
//! **A line scanner, not a parser**, and the limit is worth naming: an
//! array or an inline table spread over several lines has its key on the
//! first of them, so a value on a continuation line carries no key path.
//! That costs a key path — never a finding, since the identifiers come
//! from the same raw text either way — and it is the price of offsets
//! that are the raw document's rather than a parser's.
//!
//! Both `[table]` and `[[array of tables]]` set the prefix. The array
//! index is deliberately not tracked: TOML gives no way to know it
//! without holding the whole document, and a wrong index in a key path
//! is worse than an absent one.

use super::locate::{KeySpan, lines, value_length};

/// TOML has one comment character.
const COMMENT: &[u8] = b"#";

pub(crate) fn key_spans(text: &str) -> Vec<KeySpan> {
    let mut table = String::new();
    let mut spans = Vec::new();

    for (offset, line) in lines(text) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(name) = header(trimmed) {
            table = name.to_string();
            continue;
        }
        let Some(equals) = line.find('=') else {
            continue;
        };
        let key = line[..equals].trim().trim_matches('"');
        if key.is_empty() {
            continue;
        }
        spans.push(KeySpan {
            start: offset + equals + 1,
            end: offset + equals + 1 + value_length(&line[equals + 1..], COMMENT),
            path: if table.is_empty() {
                key.to_string()
            } else {
                format!("{table}.{key}")
            },
        });
    }
    spans
}

/// The name inside `[table]` or `[[array]]`.
fn header(trimmed: &str) -> Option<&str> {
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
    Some(
        inner
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(inner)
            .trim(),
    )
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
    fn a_value_is_named_by_its_table_and_key() {
        keyed("[service]\nid = \"abc\"\n", &[("\"abc\"", "service.id")]);
    }

    #[test]
    fn a_key_before_any_table_is_named_alone() {
        keyed("id = \"abc\"\n", &[("\"abc\"", "id")]);
    }

    #[test]
    fn an_array_of_tables_sets_the_prefix_without_an_index() {
        keyed(
            "[[rows]]\nid = 1\n[[rows]]\nid = 2\n",
            &[("1", "rows.id"), ("2", "rows.id")],
        );
    }

    #[test]
    fn a_dotted_table_header_is_kept_whole() {
        keyed("[a.b.c]\nid = 1\n", &[("1", "a.b.c.id")]);
    }

    #[test]
    fn a_quoted_key_loses_its_quotes() {
        keyed("\"my key\" = 1\n", &[("1", "my key")]);
    }

    #[test]
    fn comments_have_no_key() {
        keyed("# id = 1\nid = 2\n", &[("2", "id")]);
    }

    /// A one-line array keeps its key; the limit is a multi-line one.
    #[test]
    fn a_single_line_array_is_covered_by_its_key() {
        keyed("ids = [1, 2, 3]\n", &[("[1, 2, 3]", "ids")]);
    }

    /// **A comment is not part of the value.** The key path a value sits
    /// under is evidence — four of the five kinds ask it whether a run is
    /// what its shape suggests — so a reader that hands a trailing
    /// comment the line's key turns a refusal into a finding.
    #[test]
    fn a_trailing_comment_is_outside_the_value() {
        keyed("id = 1 # 6a7bb780a1b2c3d4e5f60718\n", &[("1", "id")]);
    }

    /// Quote-aware, because `"a # b"` is a value carrying a hash rather
    /// than a value followed by a comment. Both TOML string forms.
    #[test]
    fn a_hash_inside_a_string_belongs_to_the_value() {
        keyed("id = \"a # b\"\n", &[("\"a # b\"", "id")]);
        keyed("id = 'a # b'\n", &[("'a # b'", "id")]);
    }
}
