//! `extract_ids` — the tool that reads a document handed to it.
//!
//! It touches no filesystem. An agent already has file-read tools;
//! duplicating them here would add a path-traversal surface for no
//! capability. The tool that needs a filesystem is `ids_le_scan`.
//!
//! **Refusals come back as rows, exactly as they do on the command
//! line.** An agent that received only the identifiers this crate was
//! willing to name would conclude a document was clean when what
//! actually happened is that nothing in it could be named — which is the
//! single most expensive way for this tool to be wrong.

use serde_json::{Value, json};

use crate::extract::{self, Clock, KIND_NAMES, Options, SUPPORTED_FORMATS, resolve_format};

const DEFAULT_MAX_RESULTS: usize = 500;
const MAX_MAX_RESULTS: usize = 5000;

pub(crate) fn definition() -> Value {
    json!({
        "name": "extract_ids",
        "description": "Extract every identifier from a document — UUID (all versions), ULID, \
                        NanoID, MongoDB ObjectId and Snowflake — with its raw text, its line \
                        and column, the document's key path for it, and its validity. Where \
                        the identifier embeds a time (UUID v1/v6/v7, ULID, ObjectId, \
                        Snowflake) the decoded instant is returned as an ISO-8601 UTC string. \
                        Parses JSON, YAML, CSV, TOML, INI and dotenv for key paths; anything \
                        else is read as text, so a format is optional. A run that cannot be \
                        named is returned as a row with `valid: false` and a named reason, \
                        never dropped.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "The document text to scan." },
                "format": {
                    "type": "string",
                    "enum": SUPPORTED_FORMATS,
                    "description": "Document format. Optional — an unrecognised or absent \
                                    format reads the text directly and returns findings \
                                    without key paths.",
                },
                "filename": {
                    "type": "string",
                    "description": "Filename used to infer the format when `format` is absent, \
                                    e.g. \"config.toml\".",
                },
                "kind": {
                    "type": "string",
                    "enum": KIND_NAMES,
                    "description": "Return only one kind. This narrows the report after the \
                                    analysis, so a run that could not be assigned a kind is \
                                    not returned at all; omit it for the complete answer.",
                },
                "maxResults": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_MAX_RESULTS,
                    "default": DEFAULT_MAX_RESULTS,
                    "description": format!(
                        "Cap on returned rows (default {DEFAULT_MAX_RESULTS}). meta.truncated \
                         reports whether any were dropped."
                    ),
                },
            },
            "required": ["content"],
            "additionalProperties": false,
        },
    })
}

pub(crate) fn run(arguments: &Value, clock: Clock) -> Result<Value, String> {
    let content = arguments
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| "content is required and must be a string".to_string())?;
    let max_results = read_max_results(arguments)?;
    let kind = super::requested_kind(arguments)?;

    // Never a refusal. An agent that knows nothing about a document
    // still gets its identifiers; only the key paths depend on knowing
    // the format.
    let format = resolve_format(
        arguments.get("format").and_then(Value::as_str),
        arguments.get("filename").and_then(Value::as_str),
    );

    let mut rows = extract::extract(content, format, Options { clock, kind });
    let (named, refused) = extract::counts(&rows);

    // The `truncated` flag matters more than the cap: a silently
    // incomplete answer is wrong in the most expensive way, and this is
    // a tool whose whole job is completeness.
    let truncated = rows.len() > max_results;
    rows.truncate(max_results);

    // A refusal is not an error — the scan ran — so this is a warning
    // and `ok` stays true. It is still said out loud, because a model
    // counting only the named rows would understate the document.
    let diagnostics: Vec<Value> = (refused > 0)
        .then(|| {
            json!({
                "severity": "warning",
                "code": "refused",
                "message": format!(
                    "{refused} run(s) could not be named; each is returned with `valid: false` \
                     and a reason"
                ),
            })
        })
        .into_iter()
        .collect();

    let count = rows.len();
    Ok(super::envelope(
        "extract_ids",
        &json!({ "ids": rows, "fileType": format, "named": named, "refused": refused }),
        count,
        &diagnostics,
        truncated,
    ))
}

/// Clamp quietly, reject loudly.
///
/// A cap above the ceiling is the caller asking for everything, which is
/// answered rather than refused. A cap that is not a positive integer —
/// a fraction, a negative, a string, a zero — is a question this tool
/// cannot answer, and a silent default would hand back a truncated
/// report the caller believed was capped where they said.
fn read_max_results(arguments: &Value) -> Result<usize, String> {
    let Some(raw) = arguments.get("maxResults") else {
        return Ok(DEFAULT_MAX_RESULTS);
    };
    let Some(value) = raw.as_u64().filter(|value| *value >= 1) else {
        return Err("maxResults must be a positive integer".to_string());
    };
    Ok(usize::try_from(value)
        .unwrap_or(MAX_MAX_RESULTS)
        .min(MAX_MAX_RESULTS))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use crate::extract::FALLBACK_FORMAT;
    use crate::extract::corpus::{CORPUS_CLOCK, document};

    const CASES: &str = include_str!("../../fixtures/mcp-extract-ids.json");
    const UUID: &str = "f47ac10b-58cc-4372-a567-0e02b2c3d479";

    #[derive(Debug, Deserialize)]
    struct Case {
        name: String,
        file: Option<String>,
        content: Option<String>,
        arguments: Value,
        expected: Option<Value>,
        #[serde(rename = "expectedError")]
        expected_error: Option<String>,
    }

    fn call(arguments: &Value) -> Value {
        run(arguments, CORPUS_CLOCK).expect("a result")
    }

    /// The pinned corpus, run against a pinned clock so the
    /// plausibility window is a fact rather than the date the suite ran.
    #[test]
    fn every_pinned_case_answers_identically() {
        let cases: Vec<Case> = serde_json::from_str(CASES).expect("the corpus is valid JSON");
        assert!(!cases.is_empty(), "the corpus is empty");

        for case in cases {
            let mut arguments = case.arguments.clone();
            let content = case
                .file
                .as_deref()
                .map(document)
                .map(str::to_string)
                .or(case.content);
            if let Some(content) = content {
                arguments["content"] = json!(content);
            }

            match (case.expected, case.expected_error) {
                (_, Some(expected)) => {
                    assert_eq!(
                        run(&arguments, CORPUS_CLOCK).expect_err(&case.name),
                        expected,
                        "{}",
                        case.name
                    );
                }
                (Some(expected), None) => {
                    assert_eq!(call(&arguments), expected, "{}", case.name);
                }
                (None, None) => panic!("{} pins neither a result nor an error", case.name),
            }
        }
    }

    #[test]
    fn the_tool_name_is_pinned() {
        assert_eq!(definition()["name"], "extract_ids");
    }

    #[test]
    fn the_advertised_enums_match_what_resolves() {
        let definition = definition();
        let schema = &definition["inputSchema"]["properties"];
        for (field, expected) in [
            ("format", SUPPORTED_FORMATS.to_vec()),
            ("kind", KIND_NAMES.to_vec()),
        ] {
            let advertised: Vec<&str> = schema[field]["enum"]
                .as_array()
                .expect("an enum")
                .iter()
                .filter_map(Value::as_str)
                .collect();
            assert_eq!(advertised, expected, "{field}");
        }
    }

    #[test]
    fn a_finding_carries_its_kind_position_key_and_decode() {
        let result = call(&json!({
            "content": "{\"service\":{\"id\":\"019ff344-cc00-7abc-8def-0123456789ab\"}}",
            "format": "json",
        }));
        let found = &result["data"]["ids"][0];
        assert_eq!(found["kind"], "uuid");
        assert_eq!(found["version"], 7);
        assert_eq!(found["variant"], "rfc4122");
        assert_eq!(found["key"], "service.id");
        assert_eq!(found["line"], 1);
        assert_eq!(found["valid"], true);
        assert_eq!(found["timestamp"], "2026-08-12T00:00:00.000Z");
    }

    /// The rule this surface exists to hold: a refusal reaches the agent.
    #[test]
    fn a_refusal_is_returned_as_a_row_and_a_warning() {
        let result = call(&json!({
            "content": "{\"id\":\"00000000-0000-0000-0000-000000000000\"}",
            "format": "json",
        }));
        assert_eq!(result["ok"], true, "the scan ran");
        assert_eq!(result["data"]["ids"][0]["valid"], false);
        assert_eq!(result["data"]["ids"][0]["refused"], "nil_or_max");
        assert_eq!(result["data"]["refused"], 1);
        assert_eq!(result["diagnostics"][0]["code"], "refused");
    }

    /// An empty answer is the scan running and finding nothing.
    #[test]
    fn a_document_with_no_identifiers_is_an_ordinary_result() {
        let result = call(&json!({ "content": "no identifiers here" }));
        assert_eq!(result["ok"], true);
        assert_eq!(result["meta"]["count"], 0);
        assert_eq!(result["diagnostics"], json!([]));
    }

    #[test]
    fn an_unknown_format_falls_back_rather_than_failing() {
        let result = call(&json!({ "content": UUID, "format": "handwriting" }));
        assert_eq!(result["data"]["fileType"], FALLBACK_FORMAT);
        assert_eq!(result["data"]["ids"][0]["kind"], "uuid");
    }

    #[test]
    fn an_unknown_kind_is_refused_rather_than_returning_nothing() {
        let error =
            run(&json!({ "content": UUID, "kind": "guid" }), CORPUS_CLOCK).expect_err("a refusal");
        assert!(error.contains("guid"), "{error}");
        assert!(
            error.contains("uuid"),
            "the alternatives are named: {error}"
        );
    }

    #[test]
    fn a_kind_narrows_the_report() {
        let content = format!("a: {UUID}\nb: 01KZSM9K00ABCDEFGH12345678\n");
        let all = call(&json!({ "content": content, "format": "yaml" }));
        assert_eq!(all["meta"]["count"], 2);
        let only = call(&json!({ "content": content, "format": "yaml", "kind": "ulid" }));
        assert_eq!(only["meta"]["count"], 1);
        assert_eq!(only["data"]["ids"][0]["kind"], "ulid");
    }

    #[test]
    fn a_fractional_cap_is_refused() {
        let error = run(&json!({ "content": "x", "maxResults": 1.5 }), CORPUS_CLOCK)
            .expect_err("a refusal");
        assert_eq!(error, "maxResults must be a positive integer");
    }

    /// A silently truncated answer is the expensive kind of wrong.
    #[test]
    fn truncation_is_reported() {
        let content = format!("{UUID}\n").repeat(3);
        let result = call(&json!({ "content": content, "maxResults": 2 }));
        assert_eq!(result["meta"]["truncated"], true);
        assert_eq!(result["meta"]["count"], 2);
    }
}
