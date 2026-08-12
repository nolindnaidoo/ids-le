//! Key paths in JSON and JSONC, from a scanner rather than a parser.
//!
//! A parser would hand back decoded values and lose the offsets, and the
//! offsets are what this whole report is indexed by — the same reason
//! `urls-le` scans JSON by hand. So this walks the raw bytes, keeps a
//! stack of the containers it is inside, and emits the byte range of
//! every scalar with the dotted path that names it.
//!
//! Array elements are `[0]`, `[1]`, matching the sibling crates:
//! `orders.[2].id` rather than `orders[2].id`, because the segments are
//! joined by one rule and a special case for arrays would be a second.
//!
//! Comments and trailing commas are tolerated, so a `.jsonc` file reads.
//! A document that does not parse is not refused — a scanner has no
//! opinion on whether the braces balanced, and half a document's key
//! paths are better than none. What it *cannot* do is invent a finding:
//! the identifiers come from `candidate.rs` over the same raw text,
//! whatever this makes of the structure.

use super::locate::{KeySpan, join};

#[derive(Debug, Clone, Copy)]
struct Frame {
    array: bool,
    index: usize,
    /// Whether the next string in an object frame is a key. Set on `{`
    /// and on every `,`; cleared by `:`.
    expect_key: bool,
}

pub(crate) fn key_spans(text: &str) -> Vec<KeySpan> {
    let bytes = text.as_bytes();
    let mut frames: Vec<Frame> = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut spans = Vec::new();
    let mut at = 0;

    while at < bytes.len() {
        match bytes[at] {
            b'{' | b'[' => {
                let array = bytes[at] == b'[';
                frames.push(Frame {
                    array,
                    index: 0,
                    expect_key: !array,
                });
                path.push(if array {
                    "[0]".to_string()
                } else {
                    String::new()
                });
                at += 1;
            }
            b'}' | b']' => {
                frames.pop();
                path.pop();
                at += 1;
            }
            b':' => {
                if let Some(frame) = frames.last_mut() {
                    frame.expect_key = false;
                }
                at += 1;
            }
            b',' => {
                if let Some(frame) = frames.last_mut() {
                    frame.expect_key = !frame.array;
                    if frame.array {
                        frame.index += 1;
                        if let Some(segment) = path.last_mut() {
                            *segment = format!("[{}]", frame.index);
                        }
                    }
                }
                at += 1;
            }
            b'"' => {
                let (content, end) = read_string(bytes, at);
                let key = frames.last().is_some_and(|frame| frame.expect_key);
                if key {
                    if let Some(segment) = path.last_mut() {
                        *segment = text[content.clone()].to_string();
                    }
                } else {
                    spans.push(KeySpan {
                        start: content.start,
                        end: content.end,
                        path: join(&path),
                    });
                }
                at = end;
            }
            b'/' => at = skip_comment(bytes, at),
            byte if is_scalar_byte(byte) => {
                let start = at;
                while at < bytes.len() && is_scalar_byte(bytes[at]) {
                    at += 1;
                }
                spans.push(KeySpan {
                    start,
                    end: at,
                    path: join(&path),
                });
            }
            _ => at += 1,
        }
    }
    spans
}

/// The byte range between the quotes, and the offset just past the
/// closing one. An unterminated string runs to the end of the document,
/// which is the same answer a reader gets by looking at it.
fn read_string(bytes: &[u8], open: usize) -> (std::ops::Range<usize>, usize) {
    let mut at = open + 1;
    while at < bytes.len() {
        if bytes[at] == b'\\' {
            at += 2;
            continue;
        }
        if bytes[at] == b'"' {
            return (open + 1..at, at + 1);
        }
        at += 1;
    }
    (open + 1..bytes.len(), bytes.len())
}

/// A bare token: a number, `true`, `false`, `null`, or whatever a
/// malformed document put there.
fn is_scalar_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'+' | b'.' | b'_')
}

fn skip_comment(bytes: &[u8], at: usize) -> usize {
    match bytes.get(at + 1) {
        Some(b'/') => bytes[at..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |offset| at + offset),
        Some(b'*') => bytes
            .windows(2)
            .skip(at + 2)
            .position(|pair| pair == b"*/")
            .map_or(bytes.len(), |offset| at + offset + 4),
        // Not a comment. One byte forward, so the scan cannot stall.
        _ => at + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::locate::key_at;

    /// Each value region as `(the text it covers, its key path)`.
    fn keyed(text: &str, expected: &[(&str, &str)]) {
        let spans = key_spans(text);
        let found: Vec<(&str, &str)> = spans
            .iter()
            .map(|span| (&text[span.start..span.end], span.path.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(found, expected, "{text}");
    }

    #[test]
    fn a_string_value_is_named_by_its_key() {
        keyed(r#"{"id":"abc"}"#, &[("abc", "id")]);
    }

    #[test]
    fn nested_objects_build_a_dotted_path() {
        keyed(r#"{"a":{"b":{"c":"x"}}}"#, &[("x", "a.b.c")]);
    }

    #[test]
    fn array_elements_are_indexed() {
        keyed(
            r#"{"ids":["x","y","z"]}"#,
            &[("x", "ids.[0]"), ("y", "ids.[1]"), ("z", "ids.[2]")],
        );
    }

    #[test]
    fn an_array_of_objects_keeps_both_the_index_and_the_key() {
        keyed(
            r#"{"rows":[{"id":"x"},{"id":"y"}]}"#,
            &[("x", "rows.[0].id"), ("y", "rows.[1].id")],
        );
    }

    /// A key is never a value. Without this a key that looks like an
    /// identifier would be reported as one, under itself.
    #[test]
    fn a_key_is_not_a_value_region() {
        let text = r#"{"550e8400-e29b-41d4-a716-446655440000":"x"}"#;
        let spans = key_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(&text[spans[0].start..spans[0].end], "x");
        assert_eq!(key_at(&spans, 2), None, "the key text has no key path");
    }

    /// A Snowflake arrives as a bare JSON number far more often than as
    /// a string, so bare tokens have to carry a key path too.
    #[test]
    fn a_bare_token_is_a_value_region() {
        keyed(
            r#"{"id":1536886938009600000,"ok":true,"n":null}"#,
            &[("1536886938009600000", "id"), ("true", "ok"), ("null", "n")],
        );
    }

    #[test]
    fn an_escaped_quote_does_not_end_a_string() {
        keyed(r#"{"a":"x\"y","b":"z"}"#, &[(r#"x\"y"#, "a"), ("z", "b")]);
    }

    #[test]
    fn comments_and_trailing_commas_are_tolerated() {
        keyed("{\n// a comment\n\"a\": \"x\",\n}", &[("x", "a")]);
        keyed("{/* a comment */\"a\": \"x\"}", &[("x", "a")]);
    }

    /// A scanner has no opinion on whether the braces balanced, and the
    /// key paths it did work out are worth more than nothing.
    #[test]
    fn a_truncated_document_still_yields_what_it_read() {
        keyed(r#"{"a":"x","b":"#, &[("x", "a")]);
    }

    #[test]
    fn a_root_scalar_has_no_key_path() {
        keyed(r#""x""#, &[("x", "")]);
    }

    #[test]
    fn the_spans_are_ordered_and_do_not_overlap() {
        let spans = key_spans(r#"{"a":"x","b":["y","z"]}"#);
        for pair in spans.windows(2) {
            assert!(pair[0].end <= pair[1].start, "{pair:?}");
        }
    }
}
