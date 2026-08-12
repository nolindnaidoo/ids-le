//! Where a finding sits in the document's own vocabulary.
//!
//! Every format reader here answers one question: which byte ranges hold
//! a *value*, and what is the key path of each. Nothing decides what a
//! finding is — `candidate.rs` and `policy.rs` do that, identically for
//! every format — so a reader that is wrong about a key path costs a key
//! path and never a finding.
//!
//! That is the deliberate inversion of the sibling crate. `numbers-le`
//! parses each format because the format decides what counts as a
//! number. Here the format decides only how a finding is addressed, so
//! the readers are line scanners over the raw text rather than parsers
//! over a tree — which is also the only way the offsets stay the raw
//! document's, and offsets are what the whole report is indexed by.
//!
//! **A key path is evidence, not decoration.** Four of the five kinds
//! ask it whether a run is what its shape suggests — ULID and NanoID for
//! the scheme's own name, ObjectId and Snowflake through
//! `policy::names_an_id`, and only UUID never — so a reader that silently
//! mislabels a value can turn a refusal into a finding. Each reader
//! states its limits in its own module, and none of them reconstructs a
//! document — they read as far as a line goes, and stop.

use super::format::Reader;
use super::{csv, dotenv, ini, json, toml, yaml};

/// One value region, and the key path that names it.
///
/// Ranges are non-overlapping and in document order, which is what lets
/// `key_at` be a binary search. A reader that nested them would make
/// lookups answer with an outer key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeySpan {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) path: String,
}

/// The value regions of a document, by format.
///
/// The plain-text fallback has none, and that is the honest answer
/// rather than a missing feature: a `.md` file has no keys, so its
/// findings carry no key and — by the rule in `snowflake.rs` — a bare
/// integer in one is never an identifier.
pub(crate) fn key_spans(text: &str, reader: Reader) -> Vec<KeySpan> {
    match reader {
        Reader::Json => json::key_spans(text),
        Reader::Yaml => yaml::key_spans(text),
        Reader::Toml => toml::key_spans(text),
        Reader::Ini => ini::key_spans(text),
        Reader::Dotenv => dotenv::key_spans(text),
        Reader::Csv => csv::key_spans(text),
        // Exhaustive on purpose: a reader added to `format.rs` does not
        // compile until it says what it does here.
        Reader::Text => Vec::new(),
    }
}

/// The key path covering a byte offset, if one does.
pub(crate) fn key_at(spans: &[KeySpan], offset: usize) -> Option<&str> {
    let after = spans.partition_point(|span| span.start <= offset);
    let span = spans.get(after.checked_sub(1)?)?;
    (offset < span.end).then_some(span.path.as_str())
}

/// A line scanner's shared shape: every line of a document with the byte
/// offset it starts at.
///
/// Written once because four readers need it and each getting it subtly
/// wrong — a `\r\n` counted as one byte, a last line without a newline
/// dropped — is four bugs in the same place.
pub(crate) fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0;
    text.split_inclusive('\n').map(move |line| {
        let start = offset;
        offset += line.len();
        (start, line.trim_end_matches(['\n', '\r']))
    })
}

/// How far a value runs on a line that may end in a comment.
///
/// `raw` is everything after the separator, **leading whitespace
/// included**, because whether a comment character opens a comment
/// depends on what sits in front of it. The answer is a length from the
/// start of `raw`, with trailing whitespace trimmed.
///
/// **This is a correctness rule, not tidiness.** The key path a value
/// sits under is evidence: four of the five kinds ask it whether a run is
/// what its shape suggests. A reader that lets a value region run to the
/// end of the line hands a trailing comment the line's key, and
/// `_id = 1 # 6a7bb780a1b2c3d4e5f60718` named a hash an ObjectId on the
/// strength of a key that belongs to the `1`. Three readers did that and
/// `dotenv.rs` did not, which is how it was found — so the rule is
/// written once here and every reader passes its own comment characters.
///
/// Two things keep it from cutting a value short:
///
/// - **Quotes.** `id = "a # b"` is a value carrying a hash. A backslash
///   hides the next byte inside a double-quoted string; a literal
///   `'…'` string has no escapes and ends at its own quote.
/// - **Whitespace in front.** A comment character with a non-space byte
///   before it belongs to the value — `a: b#c` is one YAML plain scalar,
///   and `A=#x` is a dotenv value that several parsers read as one. This
///   is the conservative direction: it can leave a comment attached, and
///   it can never eat a value.
pub(crate) fn value_length(raw: &str, comments: &[u8]) -> usize {
    let bytes = raw.as_bytes();
    let mut quote: Option<u8> = None;
    let mut after_space = false;
    let mut at = 0;

    while at < bytes.len() {
        let byte = bytes[at];
        at += 1;
        if quote == Some(b'"') && byte == b'\\' {
            at += 1;
            continue;
        }
        if quote == Some(byte) {
            quote = None;
            after_space = false;
            continue;
        }
        if quote.is_some() {
            after_space = false;
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
            after_space = false;
            continue;
        }
        if after_space && comments.contains(&byte) {
            // The comment character is ASCII, so `at - 1` is a boundary.
            return raw[..at - 1].trim_end().len();
        }
        after_space = byte.is_ascii_whitespace();
    }
    raw.trim_end().len()
}

/// A dotted path from its segments, skipping the empty ones a
/// document's root produces.
///
/// Takes borrowed segments rather than owned ones: both callers hold a
/// stack they are still using, and a reader that copied it to build every
/// key path would copy the whole stack once per finding.
pub(crate) fn join<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    segments
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<&str>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans() -> Vec<KeySpan> {
        vec![
            KeySpan {
                start: 5,
                end: 10,
                path: "a".to_string(),
            },
            KeySpan {
                start: 20,
                end: 30,
                path: "b.c".to_string(),
            },
        ]
    }

    #[test]
    fn an_offset_inside_a_span_finds_its_key() {
        assert_eq!(key_at(&spans(), 5), Some("a"));
        assert_eq!(key_at(&spans(), 9), Some("a"));
        assert_eq!(key_at(&spans(), 25), Some("b.c"));
    }

    /// A gap between two values — a key name, a comma, whitespace — is
    /// not part of either, and answering with the previous key would
    /// hand a finding evidence from a field it does not sit in.
    #[test]
    fn an_offset_outside_every_span_finds_nothing() {
        assert_eq!(key_at(&spans(), 0), None);
        assert_eq!(key_at(&spans(), 10), None);
        assert_eq!(key_at(&spans(), 15), None);
        assert_eq!(key_at(&spans(), 30), None);
        assert_eq!(key_at(&[], 0), None);
    }

    /// A plain-text document has no keys. There is no longer a case
    /// for "a format nobody recognised" — that resolves to `Text` in
    /// `format.rs` and cannot reach here as anything else.
    #[test]
    fn a_plain_text_document_has_no_spans() {
        assert!(key_spans("id = 1\n", Reader::Text).is_empty());
    }

    #[test]
    fn every_line_carries_the_offset_it_starts_at() {
        let text = "one\ntwo\nthree";
        let found: Vec<(usize, &str)> = lines(text).collect();
        assert_eq!(found, [(0, "one"), (4, "two"), (8, "three")]);
    }

    /// A file written on Windows must not shift every column on every
    /// line by one.
    #[test]
    fn a_carriage_return_is_not_part_of_a_line() {
        let found: Vec<(usize, &str)> = lines("one\r\ntwo\r\n").collect();
        assert_eq!(found, [(0, "one"), (5, "two")]);
    }

    #[test]
    fn a_trailing_newline_does_not_add_an_empty_line() {
        assert_eq!(lines("one\n").count(), 1);
        assert_eq!(lines("").count(), 0);
    }

    const HASH: &[u8] = b"#";

    /// The rule three readers were missing. A trailing comment is not
    /// part of the value, and the value keeps no trailing whitespace.
    #[test]
    fn a_comment_ends_the_value() {
        assert_eq!(value_length(" 1 # a comment", HASH), 2);
        assert_eq!(value_length(" 1\t# a comment", HASH), 2);
        assert_eq!(value_length(" 1", HASH), 2);
        assert_eq!(value_length("", HASH), 0);
    }

    /// The reason it cannot be `find('#')`: quoting exists so a value can
    /// hold the comment character.
    #[test]
    fn a_comment_character_inside_a_string_is_part_of_the_value() {
        assert_eq!(value_length(r#" "a # b""#, HASH), 8);
        assert_eq!(value_length(" 'a # b'", HASH), 8);
        assert_eq!(value_length(r#" "a # b" # gone"#, HASH), 8);
        // A backslash hides the closing quote of a double-quoted string,
        // so the `#` after it is still inside.
        assert_eq!(value_length(r#" "a\" # b""#, HASH), 10);
    }

    /// Whitespace has to sit in front of it. `a: b#c` is one YAML plain
    /// scalar, and the conservative direction can never eat a value.
    #[test]
    fn a_comment_character_with_no_space_before_it_is_part_of_the_value() {
        assert_eq!(value_length(" b#c", HASH), 4);
        assert_eq!(value_length("#ff0000", HASH), 7);
    }

    /// Each reader brings its own dialect's characters.
    #[test]
    fn a_reader_chooses_which_characters_open_a_comment() {
        assert_eq!(value_length(" 1 ; two", b";#"), 2);
        assert_eq!(value_length(" 1 ; two", HASH), 8, "not this dialect's");
    }

    #[test]
    fn a_value_that_is_only_a_comment_has_no_length() {
        assert_eq!(value_length(" # all of it", HASH), 0);
    }

    #[test]
    fn a_path_skips_the_segments_a_root_leaves_empty() {
        assert_eq!(join(["a", "b"]), "a.b");
        assert_eq!(join(["", "b"]), "b");
        assert_eq!(join([""; 0]), "");
    }
}
