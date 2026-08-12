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
//! **A key path is evidence, not decoration.** Three of the five kinds
//! ask it whether a run is what its shape suggests, so a reader that
//! silently mislabels a value can turn a refusal into a finding. Each
//! reader states its limits in its own module, and none of them
//! reconstructs a document — they read as far as a line goes, and stop.

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
pub(crate) fn key_spans(text: &str, format: &str) -> Vec<KeySpan> {
    match format {
        "json" => json::key_spans(text),
        "yaml" => yaml::key_spans(text),
        "toml" => toml::key_spans(text),
        "ini" => ini::key_spans(text),
        "env" => dotenv::key_spans(text),
        "csv" => csv::key_spans(text),
        _ => Vec::new(),
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

/// A dotted path from its segments, skipping the empty ones a
/// document's root produces.
pub(crate) fn join(segments: &[String]) -> String {
    segments
        .iter()
        .filter(|segment| !segment.is_empty())
        .cloned()
        .collect::<Vec<String>>()
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

    #[test]
    fn a_document_of_an_unreadable_format_has_no_spans() {
        assert!(key_spans("id = 1\n", "text").is_empty());
        assert!(key_spans("id = 1\n", "nonsense").is_empty());
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

    #[test]
    fn a_path_skips_the_segments_a_root_leaves_empty() {
        assert_eq!(join(&["a".to_string(), "b".to_string()]), "a.b");
        assert_eq!(join(&[String::new(), "b".to_string()]), "b");
        assert_eq!(join(&[]), "");
    }
}
