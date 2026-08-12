//! Key paths in `.env` files: one key, one value, one line.
//!
//! The simplest reader here, and the one whose key names carry the most
//! weight — `.env` is where `DISCORD_CHANNEL_ID=` lives, and that name
//! is the whole reason a bare integer beside it can be named at all.

use super::locate::{KeySpan, lines};

pub(crate) fn key_spans(text: &str) -> Vec<KeySpan> {
    lines(text).filter_map(span_of).collect()
}

fn span_of((offset, line): (usize, &str)) -> Option<KeySpan> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let equals = line.find('=')?;
    let declared = line[..equals].trim();
    let key = declared.strip_prefix("export ").unwrap_or(declared).trim();
    if key.is_empty() {
        return None;
    }

    let raw = &line[equals + 1..];
    Some(KeySpan {
        start: offset + equals + 1,
        end: offset + equals + 1 + value_length(raw),
        path: key.to_string(),
    })
}

/// How far the value runs.
///
/// A `#` inside a quoted value is part of the value — that is the whole
/// reason quoting exists in these files — so the comment is only cut off
/// an unquoted one.
fn value_length(raw: &str) -> usize {
    let leading = raw.len() - raw.trim_start().len();
    let body = raw.trim_start();
    if body.starts_with('"') || body.starts_with('\'') {
        return raw.len();
    }
    leading + body.find(" #").unwrap_or(body.len())
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
        keyed("PORT=8080\n", &[("8080", "PORT")]);
    }

    #[test]
    fn an_export_prefix_is_removed_from_the_key() {
        keyed(
            "export DISCORD_CHANNEL_ID=1\n",
            &[("1", "DISCORD_CHANNEL_ID")],
        );
    }

    #[test]
    fn comments_and_blank_lines_have_no_key() {
        keyed("# ID=1\n\n   \nA=2\n", &[("2", "A")]);
    }

    #[test]
    fn an_inline_comment_is_outside_an_unquoted_value() {
        let text = "A=2 # the id 550e8400-e29b-41d4-a716-446655440000\n";
        let spans = key_spans(text);
        assert_eq!(&text[spans[0].start..spans[0].end], "2");
    }

    /// A `#` inside quotes belongs to the value, which is what quoting
    /// is for.
    #[test]
    fn a_quoted_value_keeps_its_hash() {
        keyed(r#"A="a#b""#, &[(r#""a#b""#, "A")]);
    }

    #[test]
    fn a_line_with_no_equals_is_not_a_value() {
        keyed("JUST_A_WORD\n", &[]);
    }

    #[test]
    fn the_spans_are_ordered_and_do_not_overlap() {
        let spans = key_spans("A=1\nB=2\nC=3\n");
        assert_eq!(spans.len(), 3);
        for pair in spans.windows(2) {
            assert!(pair[0].end <= pair[1].start, "{pair:?}");
        }
    }
}
