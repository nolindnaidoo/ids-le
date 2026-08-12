//! The pinned corpus: whole documents, whole answers.
//!
//! The unit tests in each module hold one rule each. This holds the
//! **composition** — every reader, every kind and every refusal reason
//! over real files — so a change that keeps each rule true and still
//! moves the report fails here.
//!
//! Two of the documents are the point rather than examples:
//!
//! - `ambiguous.json` is twelve runs that every one of which this crate
//!   refuses, and the corpus asserts the *reason* for each. A change
//!   that starts naming one of them is a change from "refuses rather
//!   than guesses" to "guesses", and it cannot happen quietly.
//! - `timestamps.json` pins six decodes across four schemes and three
//!   completely different bit layouts. The values were computed outside
//!   this implementation; three of them are RFC 9562's own examples,
//!   which the RFC states name one instant.
//!
//! Unlike the sibling crates there is no editor extension on the other
//! side of this corpus to be held equal to. It is a characterisation
//! record of this crate's own behaviour, and its job is that a change to
//! the report is deliberate and visible in a diff.

use super::policy::Clock;

/// The clock every corpus case is read against.
///
/// 2026-08-12T00:00:00Z, pinned. The plausibility window ends a year
/// past now, so a corpus read against the wall clock would answer
/// differently in 2028 — which would make it a test of the calendar.
pub(crate) const CORPUS_CLOCK: Clock = Clock::at(1_786_492_800_000);

/// One corpus document, by name. Panics on a name the corpus does not
/// carry — a test naming a file that is not there is a broken test, not
/// a runtime condition.
pub(crate) fn document(name: &str) -> &'static str {
    match name {
        "ids.json" => include_str!("../../fixtures/documents/ids.json"),
        "ids.yaml" => include_str!("../../fixtures/documents/ids.yaml"),
        "ids.toml" => include_str!("../../fixtures/documents/ids.toml"),
        "ids.ini" => include_str!("../../fixtures/documents/ids.ini"),
        "ids.env" => include_str!("../../fixtures/documents/ids.env"),
        "ids.csv" => include_str!("../../fixtures/documents/ids.csv"),
        "ids.txt" => include_str!("../../fixtures/documents/ids.txt"),
        "ambiguous.json" => include_str!("../../fixtures/documents/ambiguous.json"),
        "context.json" => include_str!("../../fixtures/documents/context.json"),
        "timestamps.json" => include_str!("../../fixtures/documents/timestamps.json"),
        other => panic!("the corpus has no document named {other}"),
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde_json::Value;

    use super::{CORPUS_CLOCK, document};
    use crate::extract::{Kind, Options, Reason, SUPPORTED_FORMATS, extract};

    const CORPUS: &str = include_str!("../../fixtures/extraction.json");

    #[derive(Debug, Deserialize)]
    struct Corpus {
        /// The instant `CORPUS_CLOCK` is pinned at, restated here so the
        /// fixture is readable on its own.
        now: i64,
        documents: Vec<Case>,
        /// Every run in `ambiguous.json`, with the reason it is refused.
        ambiguity: Vec<Refused>,
        /// Every decode in `timestamps.json`.
        timestamps: Vec<Decoded>,
    }

    #[derive(Debug, Deserialize)]
    struct Case {
        name: String,
        file: String,
        #[serde(rename = "fileType")]
        file_type: String,
        expected: Vec<Value>,
    }

    #[derive(Debug, Deserialize)]
    struct Refused {
        value: String,
        reason: Reason,
    }

    #[derive(Debug, Deserialize)]
    struct Decoded {
        value: String,
        kind: Kind,
        timestamp: String,
    }

    fn corpus() -> Corpus {
        serde_json::from_str(CORPUS).expect("the corpus is valid JSON")
    }

    fn options() -> Options {
        Options {
            clock: CORPUS_CLOCK,
            kind: None,
        }
    }

    fn rows(file: &str, format: &str) -> Vec<Value> {
        extract(document(file), format, options())
            .iter()
            .map(|found| serde_json::to_value(found).expect("a row serializes"))
            .collect()
    }

    #[test]
    fn the_corpus_clock_matches_the_constant() {
        assert_eq!(corpus().now, CORPUS_CLOCK.now_ms);
    }

    #[test]
    fn every_corpus_document_reproduces() {
        let corpus = corpus();
        assert!(!corpus.documents.is_empty(), "the corpus is empty");
        for case in corpus.documents {
            assert_eq!(
                rows(&case.file, &case.file_type),
                case.expected,
                "{}",
                case.name
            );
        }
    }

    /// Every reader has a document, or one of them can rot untested.
    #[test]
    fn the_corpus_covers_every_reader() {
        let corpus = corpus();
        for format in SUPPORTED_FORMATS {
            assert!(
                corpus.documents.iter().any(|case| case.file_type == format),
                "no corpus case reads {format}"
            );
        }
    }

    /// Every kind is named somewhere in the corpus, so a kind cannot be
    /// added to the enum and never produced.
    #[test]
    fn the_corpus_names_every_kind() {
        let corpus = corpus();
        let named: Vec<&Value> = corpus
            .documents
            .iter()
            .flat_map(|case| &case.expected)
            .filter(|row| row["valid"] == true)
            .collect();
        for kind in ["uuid", "ulid", "nanoid", "objectid", "snowflake"] {
            assert!(
                named.iter().any(|row| row["kind"] == kind),
                "no corpus case names a {kind}"
            );
        }
    }

    /// **The refusal contract.** Every run in the ambiguity document is
    /// refused, with the reason the corpus pins — and every reason this
    /// crate can give appears among them.
    #[test]
    fn every_ambiguity_case_is_refused_for_the_reason_it_pins() {
        let corpus = corpus();
        let found = rows("ambiguous.json", "json");
        assert_eq!(
            found.len(),
            corpus.ambiguity.len(),
            "the ambiguity document and its pinned reasons have drifted apart"
        );

        for (row, pinned) in found.iter().zip(&corpus.ambiguity) {
            assert_eq!(row["value"], pinned.value);
            assert_eq!(row["valid"], false, "{} was named", pinned.value);
            assert_eq!(
                row["refused"],
                serde_json::to_value(pinned.reason).expect("serializes"),
                "{}",
                pinned.value
            );
            assert!(
                row["detail"].as_str().is_some_and(|text| text.len() > 20),
                "{} was refused without saying why",
                pinned.value
            );
        }

        for reason in [
            Reason::AmbiguousKind,
            Reason::Malformed,
            Reason::NilOrMax,
            Reason::VersionClaimMismatch,
            Reason::TimestampImplausible,
        ] {
            assert!(
                corpus.ambiguity.iter().any(|case| case.reason == reason),
                "no ambiguity case produces {reason:?}"
            );
        }
    }

    /// **The contextual rule, pinned as a document.** The same 24 hex
    /// characters appear four times: two under fields the document names
    /// identifiers, two not. The verdict follows the field, because an
    /// ObjectId's whole specification is *24 hex digits* and the run can
    /// never settle it alone — the same reasoning that keeps a bare
    /// integer from being a Snowflake, applied to the other kind that
    /// needs it.
    ///
    /// Read as text, where there are no key paths at all, none of the
    /// four is named. That is the same document and the same bytes, so
    /// nothing about the value moved.
    #[test]
    fn the_same_run_is_named_or_refused_by_the_field_it_sits_in() {
        let keyed = rows("context.json", "json");
        let value = keyed[0]["value"].clone();
        assert!(
            keyed.iter().all(|row| row["value"] == value),
            "the document must hold one value four times"
        );

        let verdicts: Vec<(&str, bool)> = keyed
            .iter()
            .map(|row| {
                (
                    row["key"].as_str().unwrap_or_default(),
                    row["valid"] == true,
                )
            })
            .collect();
        assert_eq!(
            verdicts,
            [
                ("record._id", true),
                ("mirror.documentId", true),
                ("checksum", false),
                ("manifest.[0]", false),
            ]
        );

        for row in keyed.iter().filter(|row| row["valid"] == false) {
            assert_eq!(row["refused"], "ambiguous_kind", "{row}");
            assert!(row["kind"].is_null(), "a kind was named anyway: {row}");
            assert_eq!(
                row["timestamp"], "2026-08-12T00:00:00.000Z",
                "the decode rides on the refusal so a reader can disagree: {row}"
            );
        }

        let bare = rows("context.json", "text");
        assert_eq!(bare.len(), keyed.len(), "the same runs, without key paths");
        assert!(
            bare.iter().all(|row| row["valid"] == false),
            "a document with no keys names no field an identifier: {bare:?}"
        );
    }

    /// **The decode contract.** Six identifiers, four schemes, three
    /// unrelated bit layouts, all pinned against values computed outside
    /// this implementation.
    #[test]
    fn every_pinned_timestamp_decodes() {
        let corpus = corpus();
        let found = rows("timestamps.json", "json");
        assert_eq!(found.len(), corpus.timestamps.len());

        for (row, pinned) in found.iter().zip(&corpus.timestamps) {
            assert_eq!(row["value"], pinned.value);
            assert_eq!(row["valid"], true, "{} was refused", pinned.value);
            assert_eq!(
                row["kind"],
                serde_json::to_value(pinned.kind).expect("serializes"),
                "{}",
                pinned.value
            );
            assert_eq!(row["timestamp"], pinned.timestamp, "{}", pinned.value);
        }
    }

    /// Every kind that carries a clock has a pinned decode. A kind whose
    /// decoder was never exercised is a decoder nobody would notice
    /// breaking.
    #[test]
    fn every_time_bearing_kind_has_a_pinned_decode() {
        let corpus = corpus();
        for kind in [Kind::Uuid, Kind::Ulid, Kind::ObjectId, Kind::Snowflake] {
            assert!(
                corpus.timestamps.iter().any(|case| case.kind == kind),
                "no pinned decode for {kind:?}"
            );
        }
    }

    /// The same identifiers, six ways of writing them down. What changes
    /// between the formats is the key path, never which runs are found —
    /// which is the claim `format.rs` makes and the one most likely to
    /// stop being true.
    #[test]
    fn the_formats_agree_on_which_runs_are_identifiers() {
        let values = |file: &str, format: &str| -> Vec<Value> {
            rows(file, format)
                .into_iter()
                .map(|row| row["value"].clone())
                .collect()
        };
        assert_eq!(values("ids.json", "json"), values("ids.yaml", "yaml"));
        // TOML and INI carry a subset of the same document; every value
        // they do carry is one the JSON carries too.
        for file in [("ids.toml", "toml"), ("ids.ini", "ini")] {
            for value in values(file.0, file.1) {
                assert!(
                    values("ids.json", "json").contains(&value),
                    "{value} is in {} and not in ids.json",
                    file.0
                );
            }
        }
    }
}
