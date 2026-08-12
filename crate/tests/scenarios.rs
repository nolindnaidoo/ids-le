//! The tier that needs a document far larger than any editor opens.
//!
//! Gated behind `IDS_LE_SCENARIOS` and run by CI. Nothing here
//! substitutes for the unit tests, which run everywhere on every push.
//!
//! **A skipped scenario is never reported as a pass.**

use std::fmt::Write as _;
use std::io::Write as _;

fn enabled(name: &str) -> bool {
    if std::env::var_os("IDS_LE_SCENARIOS").is_some() {
        return true;
    }
    eprintln!("SKIPPED {name}: set IDS_LE_SCENARIOS to run it");
    false
}

fn scan(content: &str, format: &str) -> serde_json::Value {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ids-le"))
        .args(["--stdin", "--format", format])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .as_mut()
                .expect("stdin")
                .write_all(content.as_bytes())?;
            child.wait_with_output()
        })
        .expect("the binary runs");
    serde_json::from_slice(&output.stdout).expect("stdout carries JSON")
}

/// The key-path lookup is the operation that can go quadratic here:
/// every candidate searches the span list for the one covering it, and a
/// binary search is what keeps that linear-ish. A generated fixture of
/// nothing but identifiers is the real shape.
#[test]
fn a_document_of_nothing_but_identifiers_completes() {
    if !enabled("a_document_of_nothing_but_identifiers_completes") {
        return;
    }
    // Counted from 1, deliberately. A zero tail is twelve zero nibbles,
    // which is exactly the run `uuid.rs` refuses as a v4 that is plainly
    // not random — so row zero would be a refusal, and this scenario is
    // about volume rather than about that rule.
    let mut content = String::new();
    for row in 1..=50_000u32 {
        let _ = writeln!(content, "k{row}: f47ac10b-58cc-4372-a567-{row:012x}");
    }
    let report = scan(&content, "yaml");
    assert_eq!(report["summary"]["ids"], 50_000);
    assert_eq!(report["summary"]["refused"], 0);
    assert_eq!(
        report["ids"][49_999]["key"], "k50000",
        "every row keeps its own key path"
    );
}

/// A minified bundle is one very long line. Column lookup counts UTF-16
/// units from the line start, which is what turns quadratic when the
/// line never ends.
#[test]
fn a_single_long_line_completes() {
    if !enabled("a_single_long_line_completes") {
        return;
    }
    let mut content = String::from("{");
    for index in 1..=50_000u32 {
        let _ = write!(
            content,
            "\"k{index}\":\"f47ac10b-58cc-4372-a567-{index:012x}\","
        );
    }
    content.push_str("\"end\":0}");
    let report = scan(&content, "json");
    assert_eq!(report["summary"]["ids"], 50_000);
    assert_eq!(report["ids"][0]["line"], 1);
}

/// Prose is the worst case for the candidate scanner: almost every run
/// is a word no kind can be, and the length filter is the only thing
/// keeping it from allocating one candidate per word.
#[test]
fn a_document_of_nothing_but_prose_completes() {
    if !enabled("a_document_of_nothing_but_prose_completes") {
        return;
    }
    let content = "the quick brown fox jumped over the lazy dog\n".repeat(200_000);
    let report = scan(&content, "text");
    assert_eq!(report["summary"]["ids"], 0);
    assert_eq!(report["summary"]["refused"], 0);
}

/// Every row a refusal. Each one formats a sentence, which is the
/// allocation a report of nothing but refusals repeats most.
#[test]
fn a_document_of_nothing_but_refusals_completes() {
    if !enabled("a_document_of_nothing_but_refusals_completes") {
        return;
    }
    let mut content = String::new();
    for row in 0..20_000u32 {
        let _ = writeln!(content, "k{row}: 5d41402abc4b2a76b9719d911017c592");
    }
    let report = scan(&content, "yaml");
    assert_eq!(report["summary"]["ids"], 0);
    assert_eq!(report["summary"]["refused"], 20_000);
}
