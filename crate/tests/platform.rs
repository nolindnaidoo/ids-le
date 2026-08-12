//! Behaviour that differs by operating system, asserted rather than
//! hoped.
//!
//! Every case here is something that shipped wrong somewhere in this
//! family: a report full of `\` on Windows for a release, a suite that
//! depended on `TZ` and passed only where the variable is honoured, a
//! stdin test that raced the refusal it was asserting.
//!
//! **The time-zone case matters more here than anywhere else in the
//! family.** This crate's central claim is a decode: 48 bits out of a
//! ULID, 60 out of a UUID v1, 42 out of a Snowflake, each rendered as an
//! ISO-8601 **UTC** string. Every one of those is computed with integer
//! arithmetic against a fixed epoch and none of it may consult the
//! machine's zone — so the decodes are asserted byte-identical under
//! `TZ=UTC`, under no `TZ` at all, and under a deliberately hostile
//! `Pacific/Kiritimati` (UTC+14, and a day ahead of most of the world).
//! A suite that quietly depended on the local zone would be right on the
//! author's desk and wrong everywhere else.
//!
//! Runs on macOS, Windows and Linux. Where a platform cannot express a
//! case it is skipped **by name** on stderr, never passed quietly.

use std::fmt::Write as _;
use std::fs::File;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BINARY: &str = env!("CARGO_BIN_EXE_ids-le");
static COUNTER: AtomicUsize = AtomicUsize::new(0);
const LIMIT: Duration = Duration::from_secs(60);

/// One identifier of every kind that carries a clock, so the time-zone
/// case has four decoders to be identical about rather than one.
const UUID_V1: &str = "c232ab00-9414-11ec-b3c8-9e6bdeced846";
const UUID_V7: &str = "019ff344-cc00-7abc-8def-0123456789ab";
const ULID: &str = "01KZSM9K00ABCDEFGH12345678";
const OBJECT_ID: &str = "6a7bb780a1b2c3d4e5f60718";
const SNOWFLAKE: &str = "1536886938009600000";

struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(name: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ids-le-platform-{name}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a temporary directory");
        Self {
            root: std::fs::canonicalize(&root).expect("a canonical directory"),
        }
    }

    fn text(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let target = self.root.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("a parent directory");
        }
        std::fs::write(&target, contents).expect("a file");
        target
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Run {
    code: Option<i32>,
    stdout: String,
}

/// Run the binary with the time zone named, bounded in time, with output
/// captured to a file rather than a pipe — a report longer than a pipe
/// buffer would otherwise deadlock the parent.
fn execute(args: &[&str], timezone: Option<&str>) -> Run {
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let capture = std::env::temp_dir().join(format!(
        "ids-le-platform-capture-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&capture).expect("a capture directory");
    let out = capture.join("stdout");

    let mut command = Command::new(BINARY);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(File::create(&out).expect("a stdout file"))
        .stderr(Stdio::null());
    match timezone {
        Some(zone) => command.env("TZ", zone),
        None => command.env_remove("TZ"),
    };

    let mut child = command.spawn().expect("the binary runs");
    let started = Instant::now();
    let status = loop {
        match child.try_wait().expect("the child can be waited on") {
            Some(status) => break status,
            None if started.elapsed() >= LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the run hung past {LIMIT:?}: {args:?}");
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };

    let stdout = String::from_utf8_lossy(&std::fs::read(&out).unwrap_or_default()).into_owned();
    let _ = std::fs::remove_dir_all(&capture);
    Run {
        code: status.code(),
        stdout,
    }
}

fn run(args: &[&str]) -> Run {
    execute(args, Some("UTC"))
}

fn reports(run: &Run) -> Vec<serde_json::Value> {
    run.stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("stdout carries only JSON"))
        .collect()
}

fn skipped(case: &str, why: &str) {
    eprintln!("SKIPPED {case}: {why}");
}

/// A document holding one identifier of every time-bearing kind, under
/// keys that name the platform where the kind needs one.
fn decodable() -> String {
    format!(
        "service:\n  legacy_id: {UUID_V1}\n  request_id: {UUID_V7}\nsession:\n  ulid: {ULID}\n\
         documents:\n  - _id: {OBJECT_ID}\ndiscord:\n  channel_id: {SNOWFLAKE}\n"
    )
}

/// A tree with a nested directory, so a separator has somewhere to show
/// up.
fn nested(name: &str) -> Tree {
    let tree = Tree::new(name);
    tree.write("ids.yaml", &decodable());
    tree.write(
        "src/deep/session.json",
        &format!("{{\"ulid\":\"{ULID}\"}}\n"),
    );
    tree.write("config/app.toml", &format!("id = \"{UUID_V7}\"\n"));
    tree
}

/// **Every path in the report uses `/`, on every platform.** A sibling
/// shipped `\` on Windows for a release, which made every path in a
/// Windows report differ from the same path in a Linux one for no reason
/// a reader could see. A report is diffed against one produced somewhere
/// else; that is most of what a report in CI is for.
///
/// On Unix this passes by construction. It is the Windows leg that is
/// the check, which is why the job runs on all three.
#[test]
fn every_path_in_the_report_is_separated_by_forward_slashes() {
    let tree = nested("separators");
    let outcome = run(&[&tree.text()]);
    assert_eq!(outcome.code, Some(0));
    let scanned = reports(&outcome);
    assert_eq!(scanned.len(), 3, "the whole tree was walked");
    for report in &scanned {
        let file = report["file"].as_str().expect("a file name");
        assert!(
            !file.contains('\\'),
            "a backslash in a reported path: {file}"
        );
        assert!(
            file.contains('/'),
            "a nested path lost its separators: {file}"
        );
    }
}

/// **`TZ` independence, which is this crate's central claim.** Four
/// decoders, three epochs, one report — and none of it may move with the
/// machine's zone. Windows ignores the variable outright, so a suite
/// that depended on it would pass on two platforms and fail on the
/// third, or pass everywhere and be measuring nothing.
///
/// The whole of stdout is compared, not only the timestamps: a zone that
/// changed a column, a key or an exit code would be just as wrong.
#[test]
fn the_decoded_timestamps_do_not_depend_on_the_time_zone() {
    let tree = nested("timezone");
    let utc = execute(&[&tree.text()], Some("UTC"));
    let unset = execute(&[&tree.text()], None);
    // UTC+14, and a calendar day ahead of most of the world — the zone
    // that turns an off-by-one into a different date rather than a
    // different hour.
    let hostile = execute(&[&tree.text()], Some("Pacific/Kiritimati"));

    assert_eq!(utc.stdout, unset.stdout, "TZ=UTC differs from TZ unset");
    assert_eq!(utc.stdout, hostile.stdout, "the report moved with the zone");
    assert_eq!(utc.code, unset.code);
    assert_eq!(utc.code, hostile.code);

    // And the decodes are the ones the corpus pins, so this is a check
    // on the arithmetic rather than on three runs agreeing about
    // something wrong.
    let decoded: Vec<String> = reports(&utc)
        .iter()
        .flat_map(|report| report["ids"].as_array().expect("rows").clone())
        .filter_map(|row| row["timestamp"].as_str().map(str::to_string))
        .collect();
    assert!(
        decoded.len() >= 5,
        "the fixture stopped exercising the decoders: {decoded:?}"
    );
    assert!(
        decoded.iter().all(|stamp| stamp.ends_with('Z')),
        "a decode is not UTC: {decoded:?}"
    );
    for pinned in ["2022-02-22T19:22:22.000Z", "2026-08-12T00:00:00.000Z"] {
        assert!(
            decoded.iter().any(|stamp| stamp == pinned),
            "{pinned} is not among the decodes: {decoded:?}"
        );
    }
}

/// **Case-insensitive filesystems.** `IDS.yaml` and `ids.yaml` are one
/// file on macOS and Windows and two on Linux. Either answer is right;
/// reporting one file twice is not.
#[test]
fn a_file_is_never_reported_twice_on_a_case_insensitive_filesystem() {
    let tree = Tree::new("case");
    tree.write("ids.yaml", &format!("id: {UUID_V7}\n"));
    tree.write("IDS.YAML", &format!("id: {ULID}\n"));

    let outcome = run(&[&tree.text()]);
    let named: Vec<String> = reports(&outcome)
        .iter()
        .filter_map(|report| report["file"].as_str())
        .map(str::to_string)
        .collect();

    if named.len() == 1 {
        eprintln!("case-insensitive filesystem: the two names are one file");
    }
    let mut unique = named.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        named.len(),
        "a file was reported twice: {named:?}"
    );
    assert!(
        named.len() <= 2,
        "more report lines than files written: {named:?}"
    );
}

/// **Reserved Windows filenames.** `CON`, `PRN`, `AUX`, `NUL` and `COM1`
/// cannot be created there. The assertion is that the walk survives
/// whatever the filesystem allowed — not that the files exist, which is
/// the mistake that makes a test red on one platform and vacuous on the
/// others.
#[test]
fn the_walk_survives_the_reserved_windows_filenames() {
    let tree = Tree::new("reserved");
    tree.write("ids.yaml", &format!("id: {UUID_V7}\n"));

    let mut made = Vec::new();
    for reserved in ["CON", "PRN", "AUX", "NUL", "COM1"] {
        match std::fs::write(tree.root.join(reserved), format!("id: {ULID}\n")) {
            Ok(()) => made.push(reserved),
            Err(_) => skipped(
                &format!("a file named {reserved}"),
                "this filesystem reserves the name",
            ),
        }
    }

    let outcome = run(&[&tree.text()]);
    let code = outcome.code.expect("an exit code, not a signal");
    assert!((0..=2).contains(&code), "exit {code}");
    let named: Vec<String> = reports(&outcome)
        .iter()
        .filter_map(|report| report["file"].as_str())
        .map(|file| file.rsplit('/').next().unwrap_or(file).to_string())
        .collect();
    assert!(
        named.iter().any(|file| file == "ids.yaml"),
        "the reserved names took the rest of the tree with them: {named:?}\n\
         created: {made:?}"
    );
}

/// **CRLF.** A file written on Windows ends every line with two bytes,
/// and a reader that counts the `\r` as content shifts the column of
/// everything after it — or, worse, carries it into the value and turns
/// a valid identifier into a refusal. Line numbers, columns, key paths
/// and verdicts must all be the same as the LF copy.
#[test]
fn windows_line_endings_do_not_change_the_report() {
    let tree = Tree::new("crlf");
    let document = decodable();
    let unix = tree.write("unix.yaml", &document);
    let windows = tree.write("windows.yaml", &document.replace('\n', "\r\n"));

    let rows = |path: &PathBuf| -> serde_json::Value {
        let outcome = run(&[&path.to_string_lossy()]);
        assert_eq!(outcome.code, Some(0));
        reports(&outcome)[0]["ids"].clone()
    };
    let expected = rows(&unix);
    assert_eq!(
        expected.as_array().expect("rows").len(),
        5,
        "the fixture stopped holding five identifiers"
    );
    assert_eq!(
        rows(&windows),
        expected,
        "a carriage return moved the report"
    );

    // A lone `\r` is not a line ending — `\n` is what starts a line —
    // and the whole document is therefore one line. Pinned rather than
    // improved: a classic Mac file is not a thing this tool meets, and
    // guessing at it would be inventing a line count.
    let old_mac = tree.write("mac.yaml", &document.replace('\n', "\r"));
    let single = rows(&old_mac);
    assert!(
        single
            .as_array()
            .expect("rows")
            .iter()
            .all(|row| row["line"] == 1),
        "a lone carriage return started a line: {single}"
    );
}

/// **stdin closed early.** The child refuses its arguments and exits
/// before reading a byte, so the parent's write races the refusal — on a
/// good day it succeeds, on a bad one it is a broken pipe. **Assert the
/// exit code, never the write.** That race cost a red CI once.
#[test]
fn a_child_that_refuses_before_reading_stdin_still_exits_two() {
    let mut child = Command::new(BINARY)
        // --stdin takes no file arguments: refused by the parser, before
        // anything reads a byte.
        .args(["--stdin", "unexpected.json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary runs");

    // Deliberately unchecked: a broken pipe here means the child refused
    // faster than this loop wrote, which is the behaviour under test.
    let _ = child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(&vec![b'x'; 1 << 20]);
    drop(child.stdin.take());

    assert_eq!(child.wait().expect("the child finishes").code(), Some(2));
}

/// A document arriving on stdin is read whole, on every platform —
/// including the one where a pipe is not a file descriptor.
#[test]
fn a_document_on_stdin_is_read_whole() {
    let mut child = Command::new(BINARY)
        .args(["--stdin", "--format", "yaml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary runs");

    let mut document = String::new();
    for row in 1..=10_000u32 {
        let _ = writeln!(document, "k{row}: f47ac10b-58cc-4372-a567-{row:012x}");
    }
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(document.as_bytes())
        .expect("the child is still reading");
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("the child finishes");
    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout carries JSON");
    assert_eq!(
        report["summary"]["ids"], 10_000,
        "a document arriving in pieces was read short"
    );
    assert_eq!(report["file"], "<stdin>");
}

/// An empty stdin is a document with nothing in it, not a hang and not a
/// malformed question.
#[test]
fn a_closed_stdin_is_an_empty_document() {
    let mut child = Command::new(BINARY)
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary runs");
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("the child finishes");
    assert_eq!(output.status.code(), Some(1), "nothing found is exit 1");
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout carries JSON");
    assert_eq!(
        report["summary"],
        serde_json::json!({ "ids": 0, "refused": 0 })
    );
}

/// The MCP server reads stdin to end of stream. A client that went away
/// before saying anything is a clean exit, not a hang.
#[test]
fn the_mcp_server_exits_cleanly_when_stdin_closes_immediately() {
    let mut child = Command::new(BINARY)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary runs");
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("the child finishes");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout was {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// A path the caller typed comes back the way the caller typed it. A
/// tool that helpfully rewrites the path it was given makes its own
/// messages impossible to grep for.
#[test]
fn a_refusal_names_the_path_the_caller_gave_it() {
    let tree = Tree::new("named-path");
    let missing = tree.root.join("not-here.json");
    let given = missing.to_string_lossy().into_owned();

    let output = Command::new(BINARY)
        .arg(&given)
        .stdin(Stdio::null())
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&given),
        "the refusal rewrote the path it was given\n  given: {given}\n  said:  {stderr}"
    );
    assert!(output.stdout.is_empty(), "a refusal wrote to stdout");
}

/// The suite's own tree helper is only correct if `Path::join` and the
/// walk agree about what a nested file is called. Asserted directly, so
/// a platform where they do not fails here rather than three cases
/// later with a confusing message.
#[test]
fn a_nested_file_written_by_this_suite_is_found_by_the_walk() {
    let tree = nested("sanity");
    let found: Vec<String> = reports(&run(&[&tree.text()]))
        .iter()
        .filter_map(|report| report["file"].as_str())
        .map(|file| file.rsplit('/').next().unwrap_or(file).to_string())
        .collect();
    for expected in ["ids.yaml", "session.json", "app.toml"] {
        assert!(
            found.iter().any(|file| file == expected),
            "{expected} was written and not walked: {found:?}"
        );
    }
}

/// Awkward file names, which every filesystem disagrees about. The
/// assertion is that whatever it accepted is walked and reported, and a
/// name it refused is skipped by name.
#[test]
fn awkward_file_names_are_walked() {
    let tree = Tree::new("names");
    let mut written = Vec::new();
    for name in [
        "with space.yaml",
        "\u{fc}nicode.yaml",
        "\u{1f389}.yaml",
        "trailing.dots..yaml",
    ] {
        match std::fs::write(tree.root.join(name), format!("id: {UUID_V7}\n")) {
            Ok(()) => written.push(name),
            Err(_) => skipped("an awkward file name", name),
        }
    }
    assert!(
        !written.is_empty(),
        "this filesystem refused every awkward name"
    );

    let outcome = run(&[&tree.text()]);
    assert_eq!(outcome.code, Some(0));
    let named: Vec<String> = reports(&outcome)
        .iter()
        .filter_map(|report| report["file"].as_str())
        .map(|file| file.rsplit('/').next().unwrap_or(file).to_string())
        .collect();
    for name in written {
        assert!(named.iter().any(|file| file == name), "{name}: {named:?}");
    }
}

/// A relative path the caller typed is reported as the caller typed it,
/// rather than absolutised. The report is read by someone who does not
/// have the tree, and `src/config.json` travels where
/// `/home/runner/work/…/src/config.json` does not.
#[test]
fn a_relative_path_is_reported_relative() {
    let tree = Tree::new("relative");
    tree.write("src/config.json", &format!("{{\"id\":\"{UUID_V7}\"}}\n"));

    let output = Command::new(BINARY)
        .arg("src")
        .current_dir(&tree.root)
        .stdin(Stdio::null())
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .expect("a report line")
            .as_bytes(),
    )
    .expect("stdout carries JSON");
    assert_eq!(report["file"], "src/config.json");
}
