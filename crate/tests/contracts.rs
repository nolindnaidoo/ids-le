//! The exit codes and the stdout contract, driven against the built
//! binary.
//!
//! These are the API: a shell branches on the exit code and parses
//! stdout, so both are pinned here rather than inferred from unit tests
//! of the functions behind them. Nothing here needs a network or a
//! privileged filesystem operation, so it runs everywhere on every push.
//!
//! **A new refusal adds its case here.**

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const BINARY: &str = env!("CARGO_BIN_EXE_ids-le");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

const UUID_V4: &str = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
const UUID_V7: &str = "019ff344-cc00-7abc-8def-0123456789ab";
const NIL: &str = "00000000-0000-0000-0000-000000000000";
const ULID: &str = "01KZSM9K00ABCDEFGH12345678";
const OBJECT_ID: &str = "6a7bb780a1b2c3d4e5f60718";

struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(name: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ids-le-contract-{name}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a temporary directory");
        Self {
            root: std::fs::canonicalize(&root).expect("a canonical directory"),
        }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        self.write_bytes(relative, contents.as_bytes())
    }

    fn write_bytes(&self, relative: &str, contents: &[u8]) -> PathBuf {
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
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(args: &[&str]) -> Run {
    let output = Command::new(BINARY)
        .args(args)
        .output()
        .expect("the binary runs");
    Run {
        code: output.status.code().expect("an exit code"),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Every line of stdout, parsed. Doubles as the assertion that stdout is
/// JSON Lines and nothing else — a stray human message there would fail
/// to parse.
fn reports(run: &Run) -> Vec<serde_json::Value> {
    run.stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("stdout carries only JSON"))
        .collect()
}

/// A keyed config, a source file with no keys, and prose with nothing in
/// it.
fn audit_tree(name: &str) -> Tree {
    let tree = Tree::new(name);
    tree.write("config.json", &format!("{{\"id\":\"{UUID_V4}\"}}\n"));
    tree.write("session.yaml", &format!("ulid: {ULID}\n"));
    tree.write("NOTES.md", &format!("Shipped under {UUID_V7}.\n"));
    tree.write("empty.md", "nothing identifying in this prose at all\n");
    tree
}

#[test]
fn a_tree_with_identifiers_exits_zero() {
    let tree = audit_tree("found");
    let run = run(&[&tree.path().to_string_lossy()]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let total: u64 = reports(&run)
        .iter()
        .filter_map(|report| report["summary"]["ids"].as_u64())
        .sum();
    assert_eq!(total, 3);
}

/// grep's convention, and the reason it is worth having: finding nothing
/// is an answer, not an error.
#[test]
fn a_tree_with_none_exits_one() {
    let tree = Tree::new("none");
    tree.write("docs/a.md", "no identifiers here\n");
    let run = run(&[&tree.path().to_string_lossy()]);
    assert_eq!(run.code, 1);
    assert!(run.stderr.contains("0 identifiers"), "{}", run.stderr);
}

/// Every report line carries its schema version, even one that found
/// nothing — a consumer branching on the shape needs something to branch
/// on.
#[test]
fn every_report_line_carries_the_schema_version() {
    let tree = audit_tree("schema");
    for report in reports(&run(&[&tree.path().to_string_lossy()])) {
        assert_eq!(report["schema"], 1, "{report}");
        assert!(report["ids"].is_array());
        assert!(report["summary"]["ids"].is_number());
        assert!(report["summary"]["refused"].is_number());
        assert!(report["diagnostics"].is_array());
    }
}

/// **The contract this crate exists for.** A refusal is a row with a
/// named reason; it does not move the exit code; `--strict` is how a
/// pipeline turns it into a failure.
#[test]
fn a_refusal_is_a_row_and_does_not_move_the_exit_code_unless_strict() {
    let tree = Tree::new("refusal");
    tree.write("config.json", &format!("{{\"id\":\"{NIL}\"}}\n"));

    let lenient = run(&[&tree.path().to_string_lossy()]);
    assert_eq!(lenient.code, 1, "nothing was named: {}", lenient.stderr);
    let report = &reports(&lenient)[0];
    assert_eq!(
        report["summary"],
        serde_json::json!({ "ids": 0, "refused": 1 })
    );
    assert_eq!(report["ids"][0]["refused"], "nil_or_max");
    assert_eq!(report["ids"][0]["valid"], false);
    assert_eq!(report["ids"][0]["value"], NIL);
    assert!(report["ids"][0]["detail"].is_string(), "the reason is said");
    assert!(
        lenient.stderr.contains("1 run refused"),
        "{}",
        lenient.stderr
    );

    let strict = run(&["--strict", &tree.path().to_string_lossy()]);
    assert_eq!(strict.code, 2, "{}", strict.stderr);
}

/// A found identifier beside a refusal still exits 0: something was
/// found, and the refusal is reported alongside it.
#[test]
fn a_refusal_beside_a_finding_still_exits_zero() {
    let tree = Tree::new("mixed");
    tree.write("a.yaml", &format!("good: {UUID_V4}\nbad: {NIL}\n"));
    let mixed = run(&[&tree.path().to_string_lossy()]);
    assert_eq!(mixed.code, 0, "{}", mixed.stderr);
    assert_eq!(
        reports(&mixed)[0]["summary"],
        serde_json::json!({ "ids": 1, "refused": 1 })
    );
    assert_eq!(run(&["--strict", &tree.path().to_string_lossy()]).code, 2);
}

/// Every refusal reason reachable from a document, driven end to end.
/// A new reason adds its case here.
#[test]
fn every_refusal_reason_reaches_the_report() {
    let tree = Tree::new("reasons");
    tree.write(
        "cases.json",
        concat!(
            "{\n",
            "  \"digest\": \"5d41402abc4b2a76b9719d911017c592\",\n",
            "  \"legacy\": \"f47ac10b-58cc-4372-1567-0e02b2c3d479\",\n",
            "  \"nil\": \"00000000-0000-0000-0000-000000000000\",\n",
            "  \"zeroed\": \"00000000-0000-4000-8000-000000000000\",\n",
            "  \"faraway\": \"7fffffff-ffff-7abc-8def-0123456789ab\"\n",
            "}\n"
        ),
    );
    let found = reports(&run(&[&tree.path().to_string_lossy()]));
    let reasons: Vec<&str> = found[0]["ids"]
        .as_array()
        .expect("rows")
        .iter()
        .filter_map(|row| row["refused"].as_str())
        .collect();
    assert_eq!(
        reasons,
        [
            "ambiguous_kind",
            "malformed",
            "nil_or_max",
            "version_claim_mismatch",
            "timestamp_implausible"
        ]
    );
}

/// The decode is the crate's main claim, so it is pinned against the
/// built binary rather than only against the library.
#[test]
fn an_embedded_timestamp_is_decoded_to_iso_8601_utc() {
    let tree = Tree::new("decode");
    tree.write(
        "ids.yaml",
        &format!("request_id: {UUID_V7}\nsession: {ULID}\ndocument_id: {OBJECT_ID}\n"),
    );
    let rows = reports(&run(&[&tree.path().to_string_lossy()]))[0]["ids"].clone();
    for index in 0..3 {
        assert_eq!(
            rows[index]["timestamp"], "2026-08-12T00:00:00.000Z",
            "row {index}: {rows}"
        );
        assert_eq!(
            rows[index]["valid"], true,
            "row {index} was refused, so its decode proves nothing: {rows}"
        );
    }
    assert_eq!(rows[0]["version"], 7);
    assert_eq!(rows[0]["variant"], "rfc4122");
    assert_eq!(rows[0]["key"], "request_id");
}

/// **The same 24 characters, two verdicts.** An ObjectId has no
/// structure to check — its whole specification is 24 hex digits — so
/// the document's word for the field is the only evidence there is. Under
/// a key naming an identifier it is named; under `checksum`, and in
/// prose where there are no keys at all, it is refused. Driven against
/// the binary because this is the rule a caller's report depends on.
#[test]
fn an_objectid_is_named_only_where_the_document_names_the_field_an_id() {
    let tree = Tree::new("objectid-context");
    tree.write(
        "record.json",
        &format!("{{\"_id\":\"{OBJECT_ID}\",\"checksum\":\"{OBJECT_ID}\"}}\n"),
    );
    tree.write("NOTES.md", &format!("The document was {OBJECT_ID}.\n"));

    let found = reports(&run(&[&tree.path().to_string_lossy()]));
    let rows = |suffix: &str| -> Vec<serde_json::Value> {
        found
            .iter()
            .find(|report| {
                report["file"]
                    .as_str()
                    .is_some_and(|file| file.ends_with(suffix))
            })
            .expect(suffix)["ids"]
            .as_array()
            .expect("rows")
            .clone()
    };

    let record = rows("record.json");
    assert_eq!(record.len(), 2, "the same value twice: {record:?}");
    assert_eq!(record[0]["key"], "_id");
    assert_eq!(record[0]["kind"], "objectid");
    assert_eq!(record[0]["valid"], true);
    assert_eq!(record[1]["key"], "checksum");
    assert_eq!(record[1]["kind"], serde_json::Value::Null);
    assert_eq!(record[1]["valid"], false);
    assert_eq!(record[1]["refused"], "ambiguous_kind");
    assert_eq!(
        record[0]["value"], record[1]["value"],
        "the rule is contextual, so the two values must be identical"
    );
    assert_eq!(
        record[1]["timestamp"], "2026-08-12T00:00:00.000Z",
        "the refusal carries the decode a reader would disagree with"
    );

    let prose = rows("NOTES.md");
    assert_eq!(prose.len(), 1);
    assert_eq!(
        prose[0]["valid"], false,
        "a document with no keys names no field an identifier"
    );
    assert_eq!(prose[0]["refused"], "ambiguous_kind");
}

/// The boundary stated in SPEC.md, driven against the binary: a bare
/// integer under a key that names an id is a Snowflake candidate; the
/// same integer in prose is not a finding at all.
#[test]
fn a_bare_integer_needs_a_key_that_names_an_id() {
    let tree = Tree::new("snowflake");
    tree.write("a.json", "{\"channel_id\":1536886938009600000}\n");
    tree.write("b.json", "{\"population\":1536886938009600000}\n");
    tree.write("c.md", "The number 1536886938009600000 is here.\n");

    let found = reports(&run(&[&tree.path().to_string_lossy()]));
    let by_file = |suffix: &str| -> serde_json::Value {
        found
            .iter()
            .find(|report| {
                report["file"]
                    .as_str()
                    .is_some_and(|file| file.ends_with(suffix))
            })
            .expect(suffix)
            .clone()
    };
    assert_eq!(by_file("a.json")["ids"][0]["kind"], "snowflake");
    assert_eq!(
        by_file("a.json")["ids"][0]["timestamp"],
        "2026-08-12T00:00:00.000Z"
    );
    assert_eq!(by_file("b.json")["summary"]["ids"], 0);
    assert_eq!(by_file("b.json")["summary"]["refused"], 0);
    assert_eq!(by_file("c.md")["ids"], serde_json::json!([]));
}

/// **A comment is not evidence.** The key path a value sits under is
/// what decides whether a 24-hex run is an ObjectId, so a reader that
/// hands a trailing comment the line's key promotes a refusal to a
/// finding. TOML, INI and YAML all did; dotenv did not, and that
/// disagreement between four readers of one document is how it surfaced.
#[test]
fn a_trailing_comment_does_not_lend_its_line_a_key() {
    let tree = Tree::new("comment-evidence");
    tree.write("a.toml", &format!("_id = 1 # {OBJECT_ID}\n"));
    tree.write("a.ini", &format!("_id = 1 ; {OBJECT_ID}\n"));
    tree.write("a.yaml", &format!("_id: 1 # {OBJECT_ID}\n"));
    tree.write("a.env", &format!("_ID=1 # {OBJECT_ID}\n"));

    let found = reports(&run(&[&tree.path().to_string_lossy()]));
    assert_eq!(found.len(), 4, "the whole tree was walked");
    for report in found {
        let rows = report["ids"].as_array().expect("rows");
        assert_eq!(rows.len(), 1, "{report}");
        assert_eq!(rows[0]["value"], OBJECT_ID, "{report}");
        assert!(
            rows[0]["key"].is_null(),
            "a comment lent the run its line's key: {report}"
        );
        assert_eq!(rows[0]["valid"], false, "{report}");
        assert_eq!(rows[0]["refused"], "ambiguous_kind", "{report}");
    }
}

/// **One rule for "the document names this field", across every kind
/// that reads the key path.** The leaf of the path is the field's own
/// name; a table called `ulids` or `snowflakes` is where a field sits,
/// not what the field is. ULID and NanoID used to read the whole path
/// while ObjectId and Snowflake read the leaf, so `ulids.count` was named
/// and `snowflakes.count` was not.
#[test]
fn a_table_named_for_a_scheme_does_not_name_the_field_in_it() {
    let tree = Tree::new("leaf-evidence");
    tree.write(
        "counts.yaml",
        &format!(
            "ulids:\n  count: {}\nnanoids:\n  count: {}\nsnowflakes:\n  count: {}\nobjectids:\n  \
             count: {OBJECT_ID}\n",
            ULID.to_lowercase(),
            "V1StGXR8xZ5jdHi6BxmyT",
            "1536886938009600000",
        ),
    );

    let report = &reports(&run(&[&tree.path().to_string_lossy()]))[0];
    assert_eq!(
        report["summary"]["ids"], 0,
        "a table's name named a field: {report}"
    );
    for row in report["ids"].as_array().expect("rows") {
        assert_eq!(row["refused"], "ambiguous_kind", "{row}");
    }
}

/// A binary file was never a text candidate: no report line, no effect
/// on the exit code. A file that *is* text and could not be decoded
/// keeps its named diagnostic and fails `--strict`.
#[test]
fn a_binary_file_is_counted_and_a_broken_text_file_is_reported() {
    let tree = Tree::new("binary");
    tree.write("ids.env", &format!("SERVICE_ID={UUID_V4}\n"));
    tree.write_bytes("logo.png", &[0x89, 0x50, 0x4e, 0x47, 0x00, 0x1a, 0x0a]);

    let clean = run(&[&tree.path().to_string_lossy()]);
    assert_eq!(clean.code, 0, "{}", clean.stderr);
    let files: Vec<String> = reports(&clean)
        .iter()
        .filter_map(|report| report["file"].as_str())
        .map(|file| file.rsplit(['/', '\\']).next().unwrap_or(file).to_string())
        .collect();
    assert_eq!(files, ["ids.env"], "the PNG produced no report line");
    assert!(
        clean.stderr.contains("1 binary file skipped"),
        "{}",
        clean.stderr
    );
    assert_eq!(
        run(&["--strict", &tree.path().to_string_lossy()]).code,
        0,
        "a binary file never fails --strict"
    );

    tree.write_bytes("notes.txt", &[0x68, 0x69, 0xff, 0xfe]);
    let broken = run(&["--strict", &tree.path().to_string_lossy()]);
    assert_eq!(broken.code, 2, "{}", broken.stderr);
    assert!(
        broken.stderr.contains("not UTF-8 text"),
        "{}",
        broken.stderr
    );
}

#[test]
fn an_unreadable_input_exits_two() {
    assert_eq!(run(&["/no/such/place-xyz"]).code, 2);
}

#[test]
fn an_unknown_flag_exits_two_and_names_itself() {
    let tree = audit_tree("badflag");
    let run = run(&["--strick", &tree.path().to_string_lossy()]);
    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("--strick"), "{}", run.stderr);
    assert!(run.stdout.is_empty(), "a refusal writes no report");
}

/// The deliberate asymmetry: a bad format is leniency that costs key
/// paths, a bad kind would be an empty report a caller reads as "clean".
#[test]
fn an_unknown_format_falls_back_and_an_unknown_kind_exits_two() {
    let tree = audit_tree("leniency");
    let format = run(&["--format", "klingon", &tree.path().to_string_lossy()]);
    assert_eq!(format.code, 0, "{}", format.stderr);
    assert!(
        reports(&format)
            .iter()
            .all(|report| report["format"] == "text")
    );

    let kind = run(&["--kind", "guid", &tree.path().to_string_lossy()]);
    assert_eq!(kind.code, 2);
    assert!(kind.stderr.contains("uuid"), "{}", kind.stderr);
}

#[test]
fn a_kind_narrows_the_report() {
    let tree = audit_tree("kind");
    let all = run(&[&tree.path().to_string_lossy()]);
    let only = run(&["--kind", "ulid", &tree.path().to_string_lossy()]);
    let total = |run: &Run| -> u64 {
        reports(run)
            .iter()
            .filter_map(|report| report["summary"]["ids"].as_u64())
            .sum()
    };
    assert_eq!((total(&all), total(&only)), (3, 1));
}

/// This tool names what is there and refuses what it cannot name. No
/// flag asks it to decide, rewrite or generate.
#[test]
fn no_flag_asks_for_a_judgment_or_a_change() {
    let tree = audit_tree("nojudgment");
    for attempt in ["--fix", "--generate", "--redact", "--best-effort"] {
        assert_eq!(
            run(&[attempt, &tree.path().to_string_lossy()]).code,
            2,
            "{attempt} was accepted"
        );
    }
}

#[test]
fn version_and_help_exit_clear() {
    let version = run(&["--version"]);
    assert_eq!(version.code, 0);
    assert!(version.stdout.contains("ids-le"));
    let help = run(&["--help"]);
    assert_eq!(help.code, 0);
    assert!(help.stdout.contains("usage: ids-le"));
    assert!(
        help.stdout.contains("grep"),
        "the exit convention is stated"
    );
}

#[test]
fn stdout_carries_only_reports_and_stderr_only_the_summary() {
    let tree = audit_tree("streams");
    let run = run(&[&tree.path().to_string_lossy()]);
    assert!(!reports(&run).is_empty());
    assert!(!run.stderr.contains('{'), "{}", run.stderr);
    assert!(run.stderr.contains("identifiers in"), "{}", run.stderr);
}

#[test]
fn a_document_on_stdin_is_scanned() {
    let mut child = Command::new(BINARY)
        .args(["--stdin", "--format", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(format!("{{\"id\":\"{UUID_V7}\"}}").as_bytes())
        .expect("written");
    let output = child.wait_with_output().expect("finishes");
    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout carries JSON");
    assert_eq!(report["file"], "<stdin>");
    assert_eq!(report["ids"][0]["value"], UUID_V7);
    assert_eq!(report["ids"][0]["key"], "id");
}

/// **The cross-surface contract.** Both surfaces call one entry point,
/// so they must answer identically for the same tree.
#[test]
fn the_cli_and_the_mcp_server_report_the_same_thing() {
    let tree = audit_tree("agreement");
    let from_cli = reports(&run(&[&tree.path().to_string_lossy()]));

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "ids_le_scan",
            "arguments": { "path": tree.path().to_string_lossy() },
        },
    });
    let mut child = Command::new(BINARY)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server starts");
    writeln!(child.stdin.as_mut().expect("stdin"), "{request}").expect("written");
    let output = child.wait_with_output().expect("finishes");
    let response: serde_json::Value = serde_json::from_slice(
        output
            .stdout
            .split(|byte| *byte == b'\n')
            .next()
            .expect("a line"),
    )
    .expect("the reply is JSON");

    let from_mcp = response["result"]["structuredContent"]["data"]["reports"]
        .as_array()
        .expect("reports")
        .clone();
    assert_eq!(from_mcp, from_cli, "the two surfaces disagree");
}

/// **A key path is evidence, so a reader that mislabels one decides a
/// verdict.** Both directions of that shipped, from one cause: `.tsv`
/// named the comma reader, so a tab row was a single column whose name
/// was every header joined by tabs. A run under `checksum` was named an
/// ObjectId; the same run under `user_id` was refused.
///
/// The property is the fix, not the two cases: for any delimiter the
/// tool claims to read, a value's key must be its own column's name.
#[test]
fn a_tab_separated_column_is_named_by_its_own_header() {
    let tree = Tree::new("tsv-keys");
    let named = tree.write(
        "named.tsv",
        "name\tuser_id\nalpha\t6a7bb780a1b2c3d4e5f60718\n",
    );
    let unnamed = tree.write(
        "unnamed.tsv",
        "name\tchecksum\nalpha\t6a7bb780a1b2c3d4e5f60718\n",
    );

    let named_run = run(&[&named.to_string_lossy()]);
    let rows = reports(&named_run);
    assert_eq!(rows[0]["ids"][0]["key"], "user_id", "{}", named_run.stdout);
    assert_eq!(rows[0]["ids"][0]["kind"], "objectid");
    assert_eq!(rows[0]["ids"][0]["valid"], true);

    let unnamed_run = run(&[&unnamed.to_string_lossy()]);
    let rows = reports(&unnamed_run);
    assert_eq!(rows[0]["ids"][0]["key"], "checksum");
    assert_eq!(rows[0]["ids"][0]["valid"], false);
    assert_eq!(rows[0]["ids"][0]["refused"], "ambiguous_kind");
}

/// A sentence is not a field name. `.conf` named the INI reader, which
/// reads `key: value` out of prose, so an English sentence supplied the
/// evidence that named a run — and the same bytes as `.txt` refused it.
#[test]
fn prose_never_supplies_the_key_that_names_a_run() {
    let tree = Tree::new("prose-keys");
    let body = "The failing request id: 6a7bb780a1b2c3d4e5f60718 was logged twice.\n";
    let as_conf = tree.write("notes.conf", body);
    let as_text = tree.write("notes.txt", body);

    let conf = reports(&run(&[&as_conf.to_string_lossy()]));
    let text = reports(&run(&[&as_text.to_string_lossy()]));
    assert_eq!(conf[0]["ids"][0]["valid"], text[0]["ids"][0]["valid"]);
    assert_eq!(conf[0]["ids"][0]["key"], text[0]["ids"][0]["key"]);
    assert_eq!(conf[0]["ids"][0]["valid"], false, "{:?}", conf[0]["ids"][0]);
}

/// **A filter may not decide a gate.** `--strict` exits 2 from
/// `summary.refused`, and `--kind` used to drop refusals before it was
/// computed — so `--strict --kind uuid` over a file of 620 refusals
/// exited 1 and reported `refused: 0`. The filter narrows which
/// identifiers are *named*; it has no standing over whether the
/// document could be read.
#[test]
fn a_kind_filter_cannot_hide_a_refusal_from_strict() {
    let tree = Tree::new("kind-strict");
    // 24 hex under a key that names nothing: refused `ambiguous_kind`,
    // and it carries `kind: null`, while the UUID beside it does not.
    let file = tree.write(
        "ids.json",
        "{\"checksum\":\"6a7bb780a1b2c3d4e5f60718\",\
          \"id\":\"f47ac10b-58cc-4372-a567-0e02b2c3d479\"}\n",
    );
    let path = file.to_string_lossy().to_string();

    assert_eq!(run(&["--strict", &path]).code, 2);
    assert_eq!(
        run(&["--strict", "--kind", "uuid", &path]).code,
        2,
        "a --kind filter hid the refusal that --strict exists to catch"
    );

    // And the filter still does its own job: the named row is narrowed.
    let filtered = run(&["--kind", "uuid", &path]);
    let report = &reports(&filtered)[0];
    assert_eq!(report["summary"]["refused"], 1, "{}", filtered.stdout);
    let named: Vec<&str> = report["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter(|row| row["valid"] == true)
        .map(|row| row["kind"].as_str().expect("a kind"))
        .collect();
    assert_eq!(named, ["uuid"]);
}
