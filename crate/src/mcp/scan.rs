//! `ids_le_scan` — the tool that reads files and directories.
//!
//! It sits beside `extract.rs` rather than inside `mod.rs` for the same
//! reason `extract.rs` does: a tool's schema and the code that honours it
//! are one thing, and a schema written a module away from its handler is
//! how the two came to disagree about strictness — this one accepted any
//! property a caller sent while `extract_ids` refused unknown ones.
//! `mod.rs` is the transport and the envelope; a tool is a module.
//!
//! **Reads the filesystem; never writes to it.** The walk is the same one
//! the command line does, through `crate::walk`, so "what this tool
//! reads" and "what the terminal reads" cannot drift apart.

use std::path::PathBuf;

use serde_json::{Value, json};

use crate::extract::{KIND_NAMES, resolve_format};
use crate::scan::{self, ScanOptions};
use crate::walk::{self, WalkOptions};

pub(crate) fn definition() -> Value {
    json!({
        "name": "ids_le_scan",
        "description": "Extract every identifier from files or directories, with the file \
                        it came from, its line and column, the document's key path for it, \
                        and the decoded time where the identifier carries one. Reads the \
                        filesystem; never writes to it. A run that cannot be named is \
                        returned as a row with `valid: false` and a named reason, never \
                        dropped.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "a file or directory to read" },
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "several files or directories, instead of `path`",
                },
                "format": {
                    "type": "string",
                    "description": "Force a format for every file instead of inferring one \
                                    per file name. An unrecognised name reads the text \
                                    directly, without key paths.",
                },
                "kind": {
                    "type": "string",
                    "enum": KIND_NAMES,
                    "description": "Return only one kind. This narrows the report after \
                                    the analysis; omit it for the complete answer.",
                },
                "hidden": {
                    "type": "boolean",
                    "default": false,
                    "description": "Walk hidden files and directories too, which is where \
                                    a dotenv file lives.",
                },
                "ignored": {
                    "type": "boolean",
                    "default": false,
                    "description": "Walk files excluded by .gitignore too.",
                },
            },
            // One of the two, never neither: a call naming nothing to
            // read is a question this tool cannot answer, and the
            // handler refuses it in the same words.
            "anyOf": [{ "required": ["path"] }, { "required": ["paths"] }],
            "additionalProperties": false,
        },
    })
}

pub(crate) fn run(arguments: &Value) -> Result<Value, String> {
    let inputs = requested_paths(arguments)?;
    let flag = |name: &str| {
        arguments
            .get(name)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let walk_options = WalkOptions {
        hidden: flag("hidden"),
        respect_ignore: !flag("ignored"),
    };
    let options = ScanOptions {
        format: arguments
            .get("format")
            .and_then(Value::as_str)
            .map(|name| resolve_format(Some(name), None)),
        ..ScanOptions::default()
    }
    .with_kind(super::requested_kind(arguments)?);

    let targets = walk::collect(&inputs, &walk_options)?;
    let scanned = targets
        .iter()
        .map(|target| scan::scan_file(target, options))
        .collect();
    // A binary file was never a text candidate, so it gets no report —
    // but the count is carried, because an agent reading `reports` as
    // the whole tree would otherwise be wrong about coverage.
    let (read, binary) = scan::partition(scanned);
    let reports: Vec<Value> = read
        .iter()
        .map(|report| serde_json::to_value(report).expect("a report serializes"))
        .collect();

    // Summed from the typed reports rather than read back out of the
    // JSON beside them: a lookup that missed would fall back to zero and
    // understate the tree without saying so.
    let ids: usize = read.iter().map(|report| report.summary.ids).sum();
    let refused: usize = read.iter().map(|report| report.summary.refused).sum();

    let mut diagnostics: Vec<Value> = read
        .iter()
        .filter(|report| report.was_skipped())
        .map(|report| {
            super::warning(
                "unreadable",
                &format!(
                    "{} could not be read, so this scan does not cover it",
                    report.file
                ),
            )
        })
        .collect();
    if refused > 0 {
        diagnostics.push(super::warning(
            "refused",
            &format!(
                "{refused} run(s) could not be named; each is in its file's report with \
                 `valid: false` and a reason"
            ),
        ));
    }

    let count = reports.len();
    Ok(super::envelope(
        "ids_le_scan",
        &json!({ "reports": reports, "ids": ids, "refused": refused, "binaryFiles": binary }),
        count,
        &diagnostics,
        false,
    ))
}

fn requested_paths(arguments: &Value) -> Result<Vec<PathBuf>, String> {
    if let Some(path) = arguments.get("path").and_then(Value::as_str) {
        return Ok(vec![PathBuf::from(path)]);
    }
    if let Some(items) = arguments.get("paths").and_then(Value::as_array) {
        let paths: Vec<PathBuf> = items
            .iter()
            .filter_map(|item| item.as_str().map(PathBuf::from))
            .collect();
        if paths.is_empty() {
            return Err("the list of paths was empty".to_string());
        }
        return Ok(paths);
    }
    Err("no file or directory was supplied to read".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempTree;

    #[test]
    fn the_tool_name_is_pinned() {
        assert_eq!(definition()["name"], "ids_le_scan");
    }

    /// The schema and the handler agree about what may be sent. A
    /// property the schema does not define is a typo a model should be
    /// told about, and the two ways of naming what to read are the two
    /// the handler actually accepts.
    #[test]
    fn the_schema_names_what_the_handler_requires() {
        let definition = definition();
        let schema = &definition["inputSchema"];
        assert_eq!(schema["additionalProperties"], false);

        let either: Vec<&str> = schema["anyOf"]
            .as_array()
            .expect("an anyOf")
            .iter()
            .filter_map(|branch| branch["required"][0].as_str())
            .collect();
        assert_eq!(either, ["path", "paths"]);
        for name in either {
            assert!(
                schema["properties"].get(name).is_some(),
                "{name} is required and not defined"
            );
        }
        assert!(run(&json!({})).is_err(), "the handler accepts neither");
    }

    /// **Every knob the schema advertises turns something.** A schema
    /// offering one that nothing reads is a lie a model acts on — and
    /// `hidden` and `ignored` had no test on this surface at all, which
    /// is the same class of gap as a schema that accepted anything.
    #[test]
    fn every_advertised_knob_changes_the_answer() {
        const UUID: &str = "f47ac10b-58cc-4372-a567-0e02b2c3d479";

        let tree = TempTree::new("mcp-scan-knobs");
        tree.mkdir(".git");
        tree.write(".gitignore", "ignored.env\n");
        tree.write("ignored.env", &format!("SERVICE_ID={UUID}\n"));
        tree.write(".env", &format!("SERVICE_ID={UUID}\n"));
        tree.write("plain.txt", &format!("id = \"{UUID}\"\n"));
        let path = tree.path().to_string_lossy().to_string();

        let ids = |arguments: &Value| -> u64 {
            run(arguments).expect("a result")["data"]["ids"]
                .as_u64()
                .expect("a count")
        };

        // The default walk leaves out both the hidden file and the
        // ignored one, so only `plain.txt` is read.
        assert_eq!(ids(&json!({ "path": path })), 1, "path");
        assert_eq!(ids(&json!({ "paths": [&path] })), 1, "paths");
        assert_eq!(ids(&json!({ "path": path, "hidden": true })), 2, "hidden");
        assert_eq!(ids(&json!({ "path": path, "ignored": true })), 2, "ignored");
        assert_eq!(ids(&json!({ "path": path, "kind": "ulid" })), 0, "kind");

        let forced = run(&json!({ "path": path, "format": "toml" })).expect("a result");
        assert_eq!(
            forced["data"]["reports"][0]["format"], "toml",
            "format did not force the reader"
        );
        assert_eq!(
            forced["data"]["reports"][0]["ids"][0]["key"], "id",
            "the forced reader supplied no key path"
        );
    }
}
