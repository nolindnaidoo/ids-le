pub(crate) mod candidate;
/// The pinned corpus is a test fixture and nothing else. Gating the
/// module rather than annotating its items is what keeps the crate free
/// of a dead-code allowance, which CI greps for.
#[cfg(test)]
pub(crate) mod corpus;
pub(crate) mod csv;
pub(crate) mod dotenv;
pub(crate) mod format;
pub(crate) mod ini;
pub(crate) mod json;
pub(crate) mod locate;
pub(crate) mod nanoid;
pub(crate) mod objectid;
pub(crate) mod policy;
pub(crate) mod position;
pub(crate) mod snowflake;
pub(crate) mod time;
pub(crate) mod toml;
pub(crate) mod ulid;
pub(crate) mod uuid;
pub(crate) mod yaml;

use serde::Serialize;

pub(crate) use format::{FALLBACK_FORMAT, SUPPORTED_FORMATS, resolve_format};
pub(crate) use policy::{Clock, KIND_NAMES, Kind, Reason, Variant, Verdict};
pub(crate) use position::{Position, PositionIndex};

/// What a caller can change about how a document is read.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Options {
    /// Now, for the timestamp-plausibility window. Passed in so this
    /// layer has no clock — see `policy.rs`.
    pub(crate) clock: Clock,
    /// One kind, when the caller asked for one.
    ///
    /// **A view, not a filter on the analysis.** Every candidate is
    /// still classified; this drops rows from the report afterwards, so
    /// a refusal that named no kind disappears under any `--kind`. The
    /// unfiltered run is the complete one, and SPEC.md says so.
    pub(crate) kind: Option<Kind>,
}

/// One identifier — or one refusal — and everything known about it.
///
/// A refusal is a row here, not an absence of one: `valid` is false,
/// `refused` names the reason, `detail` says it in a sentence, and
/// whatever *was* decoded before the refusal — a version, a variant, a
/// timestamp — is still carried, because a verdict with no evidence
/// under it is not a verdict a reader can check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Found {
    /// The scheme, or `null` where naming one is exactly what this crate
    /// refused to do.
    pub(crate) kind: Option<Kind>,
    /// The raw text, exactly as the document spells it.
    pub(crate) value: String,
    #[serde(flatten)]
    pub(crate) position: Position,
    /// The document's own name for where this sits — a dotted key path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) key: Option<String>,
    pub(crate) valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) variant: Option<Variant>,
    /// ISO-8601 UTC, where the identifier embeds a time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) refused: Option<Reason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) detail: Option<String>,
}

/// Every identifier and every refusal in a document, in document order.
pub(crate) fn extract(text: &str, format: &str, options: Options) -> Vec<Found> {
    let reader = format::canonical(format);
    let spans = locate::key_spans(text, reader);
    let index = PositionIndex::new(text);

    candidate::candidates(text)
        .into_iter()
        .filter_map(|candidate| {
            // An empty path is the JSON root and a headerless CSV: no
            // key, rather than a key that is the empty string.
            let key = locate::key_at(&spans, candidate.start).filter(|path| !path.is_empty());
            row(candidate.text, candidate.start, key, &index, options.clock)
        })
        .filter(|found| options.kind.is_none_or(|kind| found.kind == Some(kind)))
        .collect()
}

fn row(
    value: &str,
    offset: usize,
    key: Option<&str>,
    index: &PositionIndex,
    clock: Clock,
) -> Option<Found> {
    let base = Found {
        kind: None,
        value: value.to_string(),
        position: index.at(offset),
        key: key.map(str::to_string),
        valid: false,
        version: None,
        variant: None,
        timestamp: None,
        refused: None,
        detail: None,
    };

    match policy::classify(value, key, clock) {
        Verdict::Ignored => None,
        Verdict::Named(id) => Some(Found {
            kind: Some(id.kind),
            valid: true,
            version: id.version,
            variant: id.variant,
            timestamp: id.timestamp,
            ..base
        }),
        Verdict::Refused(refusal) => Some(Found {
            kind: refusal.kind,
            version: refusal.version,
            variant: refusal.variant,
            timestamp: refusal.timestamp,
            refused: Some(refusal.reason),
            detail: Some(refusal.detail),
            ..base
        }),
    }
}

/// How many rows were identifiers and how many were refusals.
///
/// Counted from the rows rather than tracked alongside them, so the two
/// numbers cannot drift from the list they describe.
pub(crate) fn counts(rows: &[Found]) -> (usize, usize) {
    let ids = rows.iter().filter(|found| found.valid).count();
    (ids, rows.len() - ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: Clock = Clock::at(1_786_492_800_000);
    const UUID: &str = "f47ac10b-58cc-4372-a567-0e02b2c3d479";

    fn plain() -> Options {
        Options {
            clock: CLOCK,
            kind: None,
        }
    }

    #[test]
    fn an_identifier_carries_its_kind_position_and_key() {
        let text = format!("{{\"service\":{{\"id\":\"{UUID}\"}}}}");
        let rows = extract(&text, "json", plain());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, Some(Kind::Uuid));
        assert_eq!(rows[0].value, UUID);
        assert_eq!(rows[0].key.as_deref(), Some("service.id"));
        assert_eq!(rows[0].position.line, 1);
        assert!(rows[0].valid);
    }

    /// The property the whole report rests on: a refusal is a row.
    #[test]
    fn a_refusal_is_a_row_with_a_named_reason() {
        let rows = extract(
            "id = \"00000000-0000-0000-0000-000000000000\"\n",
            "toml",
            plain(),
        );
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].valid);
        assert_eq!(rows[0].refused, Some(Reason::NilOrMax));
        assert!(rows[0].detail.is_some(), "the reason is said in a sentence");
    }

    #[test]
    fn the_counts_follow_the_rows() {
        let text = format!("a: {UUID}\nb: 00000000-0000-0000-0000-000000000000\n");
        let rows = extract(&text, "yaml", plain());
        assert_eq!(counts(&rows), (1, 1));
    }

    /// The same document read as text rather than as YAML: the same
    /// identifiers, and no keys.
    #[test]
    fn the_fallback_finds_the_same_identifiers_without_keys() {
        let text = format!("a: {UUID}\n");
        let keyed = extract(&text, "yaml", plain());
        let plain_text = extract(&text, FALLBACK_FORMAT, plain());
        assert_eq!(keyed.len(), plain_text.len());
        assert_eq!(keyed[0].value, plain_text[0].value);
        assert_eq!(keyed[0].key.as_deref(), Some("a"));
        assert_eq!(plain_text[0].key, None);
    }

    #[test]
    fn a_kind_filter_keeps_only_that_kind() {
        let text = format!("a: {UUID}\nb: 01KZSM9K00ABCDEFGH12345678\n");
        assert_eq!(extract(&text, "yaml", plain()).len(), 2);
        let only = extract(
            &text,
            "yaml",
            Options {
                kind: Some(Kind::Ulid),
                ..plain()
            },
        );
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].kind, Some(Kind::Ulid));
    }

    #[test]
    fn a_document_with_nothing_in_it_yields_nothing() {
        assert!(extract("", "json", plain()).is_empty());
        assert!(extract("just some prose", FALLBACK_FORMAT, plain()).is_empty());
    }

    /// The serialized shape is the API. `line`, `column` and `key` sit
    /// flat beside the rest so a caller reads one object, and the fields
    /// that do not apply are absent rather than null.
    #[test]
    fn the_report_shape_is_flat_and_omits_what_does_not_apply() {
        let rows = extract(&format!("id: {UUID}\n"), "yaml", plain());
        let json = serde_json::to_value(&rows[0]).expect("serializes");
        assert_eq!(json["kind"], "uuid");
        assert_eq!(json["line"], 1);
        assert_eq!(json["column"], 5);
        assert_eq!(json["key"], "id");
        assert_eq!(json["valid"], true);
        assert_eq!(json["version"], 4);
        assert_eq!(json["variant"], "rfc4122");
        assert!(json.get("timestamp").is_none(), "a v4 has no clock");
        assert!(json.get("refused").is_none());
    }

    /// A refusal serializes with `kind: null` where naming a kind is
    /// what was refused — present and null, because a reader has to be
    /// able to tell "no kind" from "field missing".
    #[test]
    fn a_refusal_serializes_its_reason_and_a_null_kind() {
        let rows = extract("id: 5d41402abc4b2a76b9719d911017c592\n", "yaml", plain());
        let json = serde_json::to_value(&rows[0]).expect("serializes");
        assert_eq!(json["kind"], serde_json::Value::Null);
        assert_eq!(json["valid"], false);
        assert_eq!(json["refused"], "ambiguous_kind");
        assert!(json["detail"].is_string());
    }
}
