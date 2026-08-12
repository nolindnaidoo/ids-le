//! Key paths in INI, `.cfg`, `.conf` and `.properties`: a section
//! header and one key per line.
//!
//! `;` and `#` both start a comment, because both are in use and a file
//! does not say which dialect it is. `=` separates a key from a value,
//! and `:` does too where no `=` is present — which is how a `.properties`
//! file writes it.

use super::locate::{KeySpan, lines};

pub(crate) fn key_spans(text: &str) -> Vec<KeySpan> {
    let mut section = String::new();
    let mut spans = Vec::new();

    for (offset, line) in lines(text) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        if let Some(name) = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            section = name.trim().to_string();
            continue;
        }
        let Some(separator) = separator(line) else {
            continue;
        };
        let key = line[..separator].trim();
        if key.is_empty() {
            continue;
        }
        spans.push(KeySpan {
            start: offset + separator + 1,
            end: offset + line.len(),
            path: if section.is_empty() {
                key.to_string()
            } else {
                format!("{section}.{key}")
            },
        });
    }
    spans
}

/// `=` where there is one, else `:`.
///
/// Asking for `=` first matters: a value is very often a URL, and
/// taking the first `:` in `url = https://x` would name the key
/// `url = https` and the value `//x`.
fn separator(line: &str) -> Option<usize> {
    line.find('=').or_else(|| line.find(':'))
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
    fn a_value_is_named_by_its_section_and_key() {
        keyed("[service]\nid = abc\n", &[("abc", "service.id")]);
    }

    #[test]
    fn a_key_before_any_section_is_named_alone() {
        keyed("id = abc\n", &[("abc", "id")]);
    }

    #[test]
    fn a_later_section_replaces_the_earlier_one() {
        keyed(
            "[a]\nid = 1\n[b]\nid = 2\n",
            &[("1", "a.id"), ("2", "b.id")],
        );
    }

    #[test]
    fn both_comment_characters_are_honoured() {
        keyed("; id = 1\n# id = 2\nid = 3\n", &[("3", "id")]);
    }

    /// The colon inside a URL must not become the separator. Without
    /// the `=`-first rule this key is `url = https` and this value is
    /// `//example.com`.
    #[test]
    fn an_equals_is_preferred_over_a_colon() {
        keyed(
            "url = https://example.com\n",
            &[("https://example.com", "url")],
        );
        keyed(
            "url:https://example.com\n",
            &[("https://example.com", "url")],
        );
    }

    #[test]
    fn a_line_with_no_separator_is_not_a_value() {
        keyed("[a]\njustaword\n", &[]);
    }
}
