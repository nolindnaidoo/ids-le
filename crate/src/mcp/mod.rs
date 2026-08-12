//! The agent surface: the same extraction over the Model Context
//! Protocol on stdio, so a model can ask what the identifiers in a tree
//! are rather than be handed the files and pattern-match them itself.
//!
//! Three rules this family's MCP surfaces established:
//!
//! - **An empty answer is not an error.** A document with no identifiers
//!   comes back as an ordinary result carrying `ok: true` — the scan
//!   ran. Only a malformed question is a protocol error.
//! - **`ok` reports whether the check ran, not whether the answer is
//!   yes.** A document this crate refused to name a single run in is
//!   `ok: true` with refusals in it.
//! - **Refusals speak the caller's vocabulary.** An MCP caller has no
//!   command line, so no message here mentions a flag.
//!
//! **This module is the transport and the envelope; a tool is a module.**
//! `extract.rs` and `scan.rs` each hold one tool's schema next to the code
//! that honours it — written a module apart, the two drifted, and
//! `ids_le_scan` spent a release accepting any property a caller sent
//! while `extract_ids` refused unknown ones.
//!
//! Read-only by construction: nothing on this surface writes.

pub(crate) mod extract;
pub(crate) mod scan;

use std::io::{BufRead, Write};
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::extract::{KIND_NAMES, Kind};

const PROTOCOL_VERSION: &str = "2025-06-18";

/// JSON-RPC error codes, from the spec.
const INVALID_PARAMS: i64 = -32602;
const METHOD_NOT_FOUND: i64 = -32601;

pub(crate) fn serve() -> ExitCode {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            return ExitCode::from(2);
        };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            // A frame that is not JSON has no id to answer against;
            // dropping it is the only honest option.
            continue;
        };
        let Some(response) = handle(&request) else {
            continue; // a notification: no reply
        };
        if writeln!(stdout, "{response}").is_err() || stdout.flush().is_err() {
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

fn handle(request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method")?.as_str()?;
    // Notifications carry no id and get no reply.
    id.as_ref()?;

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "ids-le", "version": env!("CARGO_PKG_VERSION") },
        })),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(request.get("params")),
        "ping" => Ok(json!({})),
        other => Err((
            METHOD_NOT_FOUND,
            format!("this server does not implement {other}"),
        )),
    };

    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message },
        }),
    })
}

fn tool_definitions() -> Value {
    json!([extract::definition(), scan::definition()])
}

/// Protocol failures (no tool named, an unknown tool) are JSON-RPC
/// errors; a tool that fails on its arguments returns a result carrying
/// `isError`, so a model reads the reason and reacts rather than
/// concluding the server is broken.
fn call_tool(params: Option<&Value>) -> Result<Value, (i64, String)> {
    let params = params.ok_or((INVALID_PARAMS, "no tool call was supplied".to_string()))?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((INVALID_PARAMS, "the tool call named no tool".to_string()))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let clock = crate::extract::Clock::at(crate::scan::now_ms());

    match name {
        "extract_ids" => Ok(match extract::run(&arguments, clock) {
            Ok(result) => tool_result(&result),
            Err(message) => tool_failure(&message),
        }),
        "ids_le_scan" => Ok(match scan::run(&arguments) {
            Ok(result) => tool_result(&result),
            Err(message) => tool_failure(&message),
        }),
        other => Err((
            INVALID_PARAMS,
            format!("this server offers no tool named {other}"),
        )),
    }
}

/// An unknown kind is refused, where an unknown format is not.
///
/// The asymmetry is the same one the command line makes: a bad format
/// costs key paths, and a bad kind would return an empty list that a
/// model reads as "this document has no identifiers".
///
/// Both tools share this rather than each parsing `kind` itself — the
/// same reasoning as `policy::names_an_id`. Two definitions of what a
/// kind argument means would drift, and the drift would show up as one
/// tool answering a question the other refused.
pub(crate) fn requested_kind(arguments: &Value) -> Result<Option<Kind>, String> {
    let Some(raw) = arguments.get("kind") else {
        return Ok(None);
    };
    let name = raw
        .as_str()
        .ok_or_else(|| "kind must be a string".to_string())?;
    Kind::from_name(name).map(Some).ok_or_else(|| {
        format!(
            "{name} is not a kind; it is one of {}",
            KIND_NAMES.join(", ")
        )
    })
}

/// The one result shape every tool returns: `{ ok, data, diagnostics,
/// meta }`.
///
/// **`ok` reports whether the check ran, not whether the answer is
/// yes.** A file full of runs this crate would not name is the answer,
/// not a failure to produce one — conflating the two would have a model
/// report a broken tool when what it actually learned is that the
/// identifiers in that file cannot be named from the file alone.
///
/// It follows that **`ok` is `true` by construction, and that is the contract rather
/// than a shortcut.** A tool that could not run on its arguments returns
/// `tool_failure` and never reaches an envelope; a tool that ran returns
/// one. Everything that can appear in `diagnostics` here is a warning
/// about what the run *found* — refusals, a file that could not be read
/// — and those are the answer, not a failure to produce one. This used
/// to be computed by looking for an `"error"` severity that no path
/// emits, which read as a live check and was not one.
pub(crate) fn envelope(
    tool: &str,
    data: &Value,
    count: usize,
    diagnostics: &[Value],
    truncated: bool,
) -> Value {
    json!({
        "ok": true,
        "data": data,
        "diagnostics": diagnostics,
        "meta": { "tool": tool, "count": count, "truncated": truncated },
    })
}

/// An MCP tool result: the envelope as text (what a model reads) and the
/// same envelope structured.
fn tool_result(envelope: &Value) -> Value {
    let text = serde_json::to_string_pretty(envelope).expect("an envelope serializes");
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": envelope,
        "isError": false,
    })
}

/// A diagnostic about what the run found. Warnings are the only kind
/// this surface produces — see `envelope`.
pub(crate) fn warning(code: &str, message: &str) -> Value {
    json!({ "severity": "warning", "code": code, "message": message })
}

/// The tool could not run on the arguments given. `isError` so a model
/// reads the message and corrects itself.
fn tool_failure(message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempTree;

    const UUID: &str = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
    const NIL: &str = "00000000-0000-0000-0000-000000000000";

    fn request(method: &str, params: &Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
    }

    fn call(name: &str, arguments: &Value) -> Value {
        handle(&request(
            "tools/call",
            &json!({ "name": name, "arguments": arguments }),
        ))
        .expect("a reply")
    }

    #[test]
    fn initialize_answers_with_the_protocol_version() {
        let response = handle(&request("initialize", &json!({}))).expect("a reply");
        assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(response["result"]["serverInfo"]["name"], "ids-le");
    }

    #[test]
    fn tools_list_offers_both_tools() {
        let response = handle(&request("tools/list", &json!({}))).expect("a reply");
        let tools = response["result"]["tools"].as_array().expect("tools");
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, ["extract_ids", "ids_le_scan"]);
    }

    #[test]
    fn a_notification_gets_no_reply() {
        let notification = json!({ "jsonrpc": "2.0", "method": "initialized" });
        assert!(handle(&notification).is_none());
    }

    #[test]
    fn an_unknown_method_is_a_protocol_error() {
        let response = handle(&request("does/not/exist", &json!({}))).expect("a reply");
        assert_eq!(response["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn an_unknown_tool_is_a_protocol_error() {
        let response = call("ids_le_generate", &json!({}));
        assert_eq!(response["error"]["code"], INVALID_PARAMS);
    }

    /// A bad argument is the tool failing on what it was given, not the
    /// server breaking — so it comes back as a result carrying isError.
    #[test]
    fn a_missing_argument_is_a_tool_failure_not_a_protocol_error() {
        let response = call("ids_le_scan", &json!({}));
        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["isError"], true);
        assert!(
            response["result"]["content"][0]["text"]
                .as_str()
                .expect("a message")
                .contains("no file or directory")
        );
    }

    #[test]
    fn the_content_tool_needs_no_filesystem() {
        let response = call("extract_ids", &json!({ "content": format!("id: {UUID}") }));
        let envelope = &response["result"]["structuredContent"];
        assert_eq!(envelope["data"]["ids"][0]["kind"], "uuid");
        assert!(envelope["data"].get("reports").is_none());
    }

    #[test]
    fn the_scan_tool_reports_what_it_found() {
        let tree = TempTree::new("mcp-scan");
        tree.write("config.toml", &format!("id = \"{UUID}\"\n"));
        let response = call(
            "ids_le_scan",
            &json!({ "path": tree.path().to_string_lossy() }),
        );
        let envelope = &response["result"]["structuredContent"];
        assert_eq!(response["result"]["isError"], false);
        assert_eq!(envelope["ok"], true);
        assert_eq!(envelope["data"]["ids"], 1);
        let found = &envelope["data"]["reports"][0]["ids"][0];
        assert_eq!(found["key"], "id");
        assert_eq!(found["line"], 1);
    }

    /// The rule this surface exists to hold, on the filesystem tool too.
    #[test]
    fn the_scan_tool_carries_refusals_and_says_so() {
        let tree = TempTree::new("mcp-refused");
        tree.write("config.toml", &format!("id = \"{NIL}\"\n"));
        let response = call(
            "ids_le_scan",
            &json!({ "path": tree.path().to_string_lossy() }),
        );
        let envelope = &response["result"]["structuredContent"];
        assert_eq!(envelope["ok"], true, "the scan ran");
        assert_eq!(envelope["data"]["refused"], 1);
        assert_eq!(
            envelope["data"]["reports"][0]["ids"][0]["refused"],
            "nil_or_max"
        );
        assert_eq!(envelope["diagnostics"][0]["code"], "refused");
    }

    #[test]
    fn the_scan_tool_narrows_to_one_kind_on_request() {
        let tree = TempTree::new("mcp-kind");
        tree.write(
            "a.yaml",
            &format!("a: {UUID}\nb: 01KZSM9K00ABCDEFGH12345678\n"),
        );
        let path = tree.path().to_string_lossy().to_string();
        let all = call("ids_le_scan", &json!({ "path": path }));
        assert_eq!(all["result"]["structuredContent"]["data"]["ids"], 2);
        let only = call("ids_le_scan", &json!({ "path": path, "kind": "ulid" }));
        assert_eq!(only["result"]["structuredContent"]["data"]["ids"], 1);
    }

    #[test]
    fn an_unknown_kind_is_a_tool_failure_rather_than_an_empty_report() {
        let tree = TempTree::new("mcp-badkind");
        tree.write("a.yaml", &format!("a: {UUID}\n"));
        let response = call(
            "ids_le_scan",
            &json!({ "path": tree.path().to_string_lossy(), "kind": "guid" }),
        );
        assert_eq!(response["result"]["isError"], true);
    }

    /// Refusals speak the caller's vocabulary: an MCP caller has no
    /// command line, so no message may name a flag.
    #[test]
    fn no_message_mentions_a_command_line_flag() {
        let definitions = serde_json::to_string(&tool_definitions()).expect("serializes");
        assert!(!definitions.contains("--"), "{definitions}");

        let tree = TempTree::new("mcp-vocabulary");
        tree.write("a.json", &format!("{{\"id\":\"{NIL}\"}}"));
        for arguments in [
            json!({}),
            json!({ "paths": [] }),
            json!({ "path": "/no/such/place-xyz" }),
            json!({ "path": tree.path().to_string_lossy() }),
            json!({ "path": tree.path().to_string_lossy(), "kind": "guid" }),
        ] {
            let rendered =
                serde_json::to_string(&call("ids_le_scan", &arguments)).expect("serializes");
            assert!(!rendered.contains("--"), "{rendered}");
        }
    }

    /// **Both tools are strict about their arguments in the same way.**
    /// A schema that accepts unknown properties tells a model its typo
    /// was fine and then answers a question it did not ask.
    /// `extract_ids` declared `additionalProperties: false` and a
    /// `required` list; `ids_le_scan` declared neither, which is a
    /// silent default on the surface with the least tolerance for one.
    #[test]
    fn every_tool_schema_refuses_unknown_properties_and_says_what_it_needs() {
        let definitions = tool_definitions();
        let tools = definitions.as_array().expect("tools");
        assert_eq!(tools.len(), 2, "a tool arrived or left");
        for tool in tools {
            let name = tool["name"].as_str().expect("a name");
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object", "{name}");
            assert_eq!(
                schema["additionalProperties"], false,
                "{name} accepts properties it does not define"
            );
            assert!(
                schema.get("required").is_some() || schema.get("anyOf").is_some(),
                "{name} does not say what it needs"
            );
        }
    }

    /// Every tool returns the same envelope, so a caller writes one
    /// reader for all of them.
    #[test]
    fn every_tool_returns_the_same_envelope_shape() {
        let tree = TempTree::new("mcp-envelope");
        tree.write("a.md", "x");
        let results = [
            call(
                "extract_ids",
                &json!({ "content": "x", "format": "markdown" }),
            ),
            call(
                "ids_le_scan",
                &json!({ "path": tree.path().to_string_lossy() }),
            ),
        ];
        for result in results {
            let envelope = &result["result"]["structuredContent"];
            assert!(envelope["ok"].is_boolean(), "{envelope}");
            assert!(!envelope["data"].is_null(), "{envelope}");
            assert!(envelope["diagnostics"].is_array(), "{envelope}");
            assert!(envelope["meta"]["tool"].is_string(), "{envelope}");
            assert!(envelope["meta"]["count"].is_number(), "{envelope}");
            assert!(envelope["meta"]["truncated"].is_boolean(), "{envelope}");
        }
    }
}
