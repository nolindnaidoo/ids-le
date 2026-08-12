//! Inputs a real machine holds and a fixture directory cannot.
//!
//! **Not a fixture directory.** Windows cannot check in a FIFO, a
//! symlink loop, a mode-000 file or a path over 260 characters, so the
//! tree is built at runtime and every case a platform cannot express
//! says so **by name** on stderr rather than passing quietly.
//!
//! Every case asserts the same floor: the process does not panic, does
//! not hang, exits 0, 1 or 2 — never on a signal — and whatever it
//! decided, it decided on stdout in JSON Lines and nowhere else.
//!
//! The cases are the shapes that have actually broken a reader in this
//! family: three invisible bytes at the head of a file read as content,
//! a text file that could not be decoded and vanished from the report
//! instead of being named, a named pipe one blocking `read` away from an
//! eternal CI job, and a minified bundle whose whole content is one line
//! — the shape that made the column lookup quadratic here until
//! `extract/position.rs` grew its checkpoint index.

use std::fmt::Write as _;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BINARY: &str = env!("CARGO_BIN_EXE_ids-le");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Generous enough that a loaded shared runner does not flake, tight
/// enough that a genuine hang is a failure rather than a job timeout
/// with no message.
const LIMIT: Duration = Duration::from_secs(60);

/// An identifier every content hazard carries, so "the file was read"
/// has an answer rather than an absence. A v7, so the row also carries a
/// decode.
const UUID: &str = "019ff344-cc00-7abc-8def-0123456789ab";

struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(name: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ids-le-hazard-{name}-{}-{unique}",
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

    fn text(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    fn mkdir(&self, relative: &str) -> PathBuf {
        let target = self.root.join(relative);
        std::fs::create_dir_all(&target).expect("a directory");
        target
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
    /// `None` when the process died on a signal, which is the failure
    /// this whole file is watching for.
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Run the binary, bounded in time, with both streams captured to files.
///
/// Files rather than pipes on purpose: a report of forty thousand
/// identifiers fills a pipe buffer, and a parent that waits before
/// draining one deadlocks — which would look exactly like the hang this
/// is here to detect.
fn execute(args: &[&str]) -> Run {
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let capture =
        std::env::temp_dir().join(format!("ids-le-capture-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&capture).expect("a capture directory");
    let out = capture.join("stdout");
    let err = capture.join("stderr");

    let mut child = Command::new(BINARY)
        .args(args)
        // Never inherit the terminal: `--stdin` reads it, and a child
        // waiting on a keyboard that is not there is a hang with an
        // innocent explanation.
        .stdin(Stdio::null())
        .stdout(File::create(&out).expect("a stdout file"))
        .stderr(File::create(&err).expect("a stderr file"))
        .spawn()
        .expect("the binary runs");

    let started = Instant::now();
    let status = loop {
        match child.try_wait().expect("the child can be waited on") {
            Some(status) => break status,
            None if started.elapsed() >= LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_dir_all(&capture);
                panic!("the run hung past {LIMIT:?}: {args:?}");
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };

    let read = |path: &Path| {
        String::from_utf8_lossy(&std::fs::read(path).unwrap_or_default()).into_owned()
    };
    let run = Run {
        code: status.code(),
        stdout: read(&out),
        stderr: read(&err),
    };
    let _ = std::fs::remove_dir_all(&capture);
    run
}

/// The floor every case shares. A signal — `code()` of `None` on Unix —
/// is the abort class this net exists to catch.
fn assert_answered(run: &Run, case: &str) {
    let code = run
        .code
        .unwrap_or_else(|| panic!("{case}: the process died on a signal, not an exit code"));
    assert!(
        (0..=2).contains(&code),
        "{case}: exit {code} is not one of grep's three\n{}",
        run.stderr
    );
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

/// A named skip. A platform that cannot express a case says so rather
/// than reporting a pass it did not earn.
fn skipped(case: &str, why: &str) {
    eprintln!("SKIPPED {case}: {why}");
}

fn utf16le_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xfe];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

/// The one report line for a file, by the name it ends with.
fn report_for(run: &Run, suffix: &str) -> serde_json::Value {
    reports(run)
        .into_iter()
        .find(|report| {
            report["file"]
                .as_str()
                .is_some_and(|file| file.ends_with(suffix))
        })
        .unwrap_or_else(|| panic!("{suffix} produced no report line\n{}", run.stdout))
}

// ---------------------------------------------------------------- content

/// The defect that silently emptied three crates in this family: three
/// invisible bytes at the head of a file, added by Notepad, by Excel and
/// by a PowerShell redirect, read as content. Left in, they shift every
/// column on the first line — which in a structured format can lose the
/// document entirely.
#[test]
fn a_byte_order_mark_does_not_move_the_reported_column() {
    let tree = Tree::new("bom");
    let plain = tree.write("plain.yaml", &format!("id: {UUID}\n"));
    let marked = tree.write("marked.yaml", &format!("\u{feff}id: {UUID}\n"));

    let row = |path: &PathBuf| -> serde_json::Value {
        let run = execute(&[&path.to_string_lossy()]);
        assert_answered(&run, "a byte-order mark");
        reports(&run)[0]["ids"][0].clone()
    };
    let without = row(&plain);
    assert_eq!(without["column"], 5);
    assert_eq!(without["key"], "id", "the key path survives");
    assert_eq!(row(&marked), without, "the mark changed the report");
}

/// **Never silently absent.** A file that looked like text and could not
/// be decoded is named in the report with a `skipped` diagnostic; what
/// is not allowed is a run that reports on its neighbours and says
/// nothing about the file it could not read. It is not a failure on its
/// own — `--strict` is how a pipeline refuses it.
#[test]
fn an_undecodable_text_file_is_named_rather_than_dropped() {
    let tree = Tree::new("undecodable");
    tree.write("good.yaml", &format!("id: {UUID}\n"));
    // Invalid UTF-8 with no NUL byte: it looked like text and was not.
    tree.write_bytes("broken.yaml", &[b'i', b'd', b':', b' ', 0xff, 0xfe]);

    let run = execute(&[&tree.text()]);
    assert_answered(&run, "an undecodable text file");
    assert_eq!(run.code, Some(0), "the good file beside it was still read");
    let report = report_for(&run, "broken.yaml");
    assert_eq!(report["diagnostics"][0]["code"], "skipped");
    assert_eq!(report["diagnostics"][0]["message"], "not UTF-8 text");
    assert_eq!(report["summary"]["ids"], 0);

    assert_eq!(
        execute(&["--strict", &tree.text()]).code,
        Some(2),
        "--strict is how you refuse it"
    );
}

/// A UTF-16 file — what Notepad writes when asked for "Unicode" — is
/// **binary**, not an undecodable text file: every second byte of ASCII
/// text is a NUL, which is ripgrep's own test and the one this crate
/// borrows. So it produces no report line, is counted on stderr, and
/// never fails `--strict`.
///
/// Worth pinning rather than assuming: the two failure modes look alike
/// from outside and are handled by different branches, and getting them
/// the wrong way round would make `--strict` exit 2 on any repository
/// holding a UTF-16 file.
#[test]
fn a_utf16_file_is_binary_by_the_nul_test() {
    let tree = Tree::new("utf16");
    tree.write("good.yaml", &format!("id: {UUID}\n"));
    tree.write_bytes("wide.yaml", &utf16le_with_bom(&format!("id: {UUID}\n")));

    let run = execute(&[&tree.text()]);
    assert_answered(&run, "a utf-16 file");
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    let named: Vec<String> = reports(&run)
        .iter()
        .filter_map(|report| report["file"].as_str())
        .map(|file| file.rsplit(['/', '\\']).next().unwrap_or(file).to_string())
        .collect();
    assert_eq!(
        named,
        ["good.yaml"],
        "the UTF-16 file produced a report line"
    );
    assert!(
        run.stderr.contains("1 binary file skipped"),
        "it was dropped rather than counted: {}",
        run.stderr
    );
    assert_eq!(
        execute(&["--strict", &tree.text()]).code,
        Some(0),
        "a binary file never fails --strict"
    );
}

/// An empty file is a report line with nothing in it, not an absence and
/// not an error. A reader counting files against a tree needs the line.
#[test]
fn an_empty_file_is_a_report_line_with_nothing_in_it() {
    let tree = Tree::new("empty");
    tree.write("empty.json", "");
    tree.write("blank.yaml", "   \n\t\n \n");

    let run = execute(&[&tree.text()]);
    assert_answered(&run, "an empty file");
    assert_eq!(run.code, Some(1), "nothing was found, which is an answer");
    for name in ["empty.json", "blank.yaml"] {
        let report = report_for(&run, name);
        assert_eq!(report["ids"], serde_json::json!([]), "{name}");
        assert_eq!(
            report["summary"],
            serde_json::json!({ "ids": 0, "refused": 0 }),
            "{name}"
        );
        assert_eq!(report["diagnostics"], serde_json::json!([]), "{name}");
    }
}

/// A minified bundle is one line several megabytes long. Every column is
/// counted from that line's start, so a lookup that re-counts from there
/// on every identifier is quadratic — 20,000 identifiers on one such
/// line took four seconds before `position.rs` grew its checkpoints. The
/// non-ASCII copy is the one that was slow: an ASCII document takes the
/// arithmetic path and never noticed.
#[test]
fn a_minified_document_of_several_megabytes_completes() {
    let tree = Tree::new("minified");
    for (name, preamble) in [
        ("ascii.json", ""),
        ("unicode.json", "\"note\":\"café 🎯\","),
    ] {
        let mut content = String::with_capacity(3_000_000);
        content.push('{');
        content.push_str(preamble);
        for index in 1..=60_000u32 {
            let _ = write!(
                content,
                "\"k{index}\":\"f47ac10b-58cc-4372-a567-{index:012x}\","
            );
        }
        content.push_str("\"end\":0}");
        assert!(content.len() > 2_500_000, "{name} is not a large document");
        let file = tree.write(name, &content);

        let started = Instant::now();
        let run = execute(&[&file.to_string_lossy()]);
        assert_answered(&run, name);
        let report = &reports(&run)[0];
        assert_eq!(report["summary"]["ids"], 60_000, "{name}");
        assert_eq!(report["ids"][0]["line"], 1, "{name}: it is all one line");
        assert_eq!(
            report["ids"][59_999]["line"], 1,
            "{name}: including the last row"
        );
        eprintln!(
            "hazards: {name} ({} bytes) in {:?}",
            content.len(),
            started.elapsed()
        );
    }
}

/// A file that is one enormous base64 blob — an inlined asset, a
/// certificate bundle, a fixture somebody pasted. Nothing in it is an
/// identifier, and that is what makes it the report's worst case: the
/// standard alphabet's `+` and `/` are not run characters, so a blob
/// shreds into hundreds of thousands of runs and a fair number of them
/// land on a length the classifier routes, where each refusal costs a
/// formatted sentence.
///
/// The bytes are generated from a fixed seed rather than repeated, so
/// the run lengths vary the way a real blob's do — a repeating pattern
/// produces one run length and tests one branch.
#[test]
fn a_file_that_is_one_enormous_base64_blob_completes() {
    const STANDARD: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    // base64url swaps the last two characters, and both of *those* are
    // run characters — so the same bytes become a handful of enormous
    // runs no kind can be, rather than a shredded pile of short ones.
    const URLSAFE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let tree = Tree::new("base64");
    for (name, alphabet) in [("blob.b64", STANDARD), ("blob.txt", URLSAFE)] {
        // xorshift64*, four lines and identical on every platform.
        let mut state = 0x1d5e_0b1d_2026_0812u64;
        let mut content = String::with_capacity(3_000_000);
        while content.len() < 3_000_000 {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let index = (state.wrapping_mul(0x2545_f491_4f6c_dd1d) % 64) as usize;
            content.push(char::from(alphabet[index]));
        }
        content.push('\n');
        let file = tree.write(name, &content);

        let started = Instant::now();
        let run = execute(&[&file.to_string_lossy()]);
        assert_answered(&run, name);
        let report = &reports(&run)[0];
        // Whatever it decided, the summary describes the rows it
        // reported and nothing else.
        let ids = report["summary"]["ids"].as_u64().expect("a count");
        let refused = report["summary"]["refused"].as_u64().expect("a count");
        let rows = report["ids"].as_array().expect("rows").len();
        assert_eq!(
            ids + refused,
            rows as u64,
            "{name}: the summary and the rows disagree"
        );
        eprintln!(
            "hazards: {name} ({} bytes) named {ids}, refused {refused}, in {:?}",
            content.len(),
            started.elapsed()
        );
    }
}

// ------------------------------------------------------------- filesystem

/// The hang this file exists for. A FIFO with no writer blocks a `read`
/// forever, and the scanner reads a whole file into a string.
#[cfg(unix)]
#[test]
fn a_fifo_does_not_block_the_run() {
    let tree = Tree::new("fifo");
    tree.write("ordinary.yaml", &format!("id: {UUID}\n"));
    let fifo = tree.path().join("pipe.yaml");
    // Shelled out rather than called through libc: `unsafe` is forbidden
    // crate-wide and a test is not an exemption.
    let made = Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .is_ok_and(|status| status.success());
    if !made {
        skipped("a fifo", "mkfifo is not available on this runner");
        return;
    }

    let walked = execute(&[&tree.text()]);
    assert_answered(&walked, "a fifo in a walked tree");
    assert_eq!(
        walked.code,
        Some(0),
        "the ordinary file beside it was still read\n{}",
        walked.stderr
    );

    let named = execute(&[&fifo.to_string_lossy()]);
    assert_answered(&named, "a fifo named directly");
    assert!(
        reports(&named).is_empty(),
        "a named pipe was read as a document"
    );
}

#[cfg(not(unix))]
#[test]
fn a_fifo_does_not_block_the_run() {
    skipped("a fifo", "Windows has no FIFO in a directory tree");
}

/// A file the filesystem refuses to open, beside one it does not. The
/// refusal is a fact about the tree, not a malformed question, so the
/// run finishes and names it.
#[cfg(unix)]
#[test]
fn a_permission_denied_file_is_named_and_does_not_end_the_run() {
    use std::os::unix::fs::PermissionsExt;

    let tree = Tree::new("denied");
    tree.write("open.yaml", &format!("id: {UUID}\n"));
    let closed = tree.write("closed.yaml", &format!("id: {UUID}\n"));
    std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o000))
        .expect("an unreadable file");
    if std::fs::read(&closed).is_ok() {
        skipped(
            "a permission-denied file",
            "this runner reads a mode-000 file anyway (root)",
        );
        return;
    }

    let run = execute(&[&tree.text()]);
    assert_answered(&run, "a permission-denied file");
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert_eq!(
        report_for(&run, "closed.yaml")["diagnostics"][0]["code"],
        "skipped"
    );
    assert_eq!(
        execute(&["--strict", &tree.text()]).code,
        Some(2),
        "--strict is how you refuse it"
    );
}

#[cfg(not(unix))]
#[test]
fn a_permission_denied_file_is_named_and_does_not_end_the_run() {
    skipped(
        "a permission-denied file",
        "Windows ACLs are not chmod; there is no mode-000 an unprivileged test can set",
    );
}

/// A symlink loop, with an ordinary file beside it. `follow_links(false)`
/// is what makes this terminate, and nothing else asserts it.
#[test]
fn a_symlink_loop_terminates() {
    let tree = Tree::new("loop");
    tree.write("ordinary.yaml", &format!("id: {UUID}\n"));
    let inner = tree.mkdir("inner");

    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(tree.path(), inner.join("up")).is_ok();
    // Windows needs Developer Mode or an elevated process to create one,
    // so a failure is the platform refusing rather than the code being
    // wrong.
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_dir(tree.path(), inner.join("up")).is_ok();
    #[cfg(not(any(unix, windows)))]
    let made = false;

    if !made {
        skipped(
            "a symlink loop",
            "this platform refused to create a symlink",
        );
        return;
    }

    let run = execute(&[&tree.text()]);
    assert_answered(&run, "a symlink loop");
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    let named: Vec<String> = reports(&run)
        .iter()
        .filter_map(|report| report["file"].as_str())
        .map(|file| file.rsplit(['/', '\\']).next().unwrap_or(file).to_string())
        .collect();
    assert_eq!(
        named,
        ["ordinary.yaml"],
        "the loop was descended rather than left alone"
    );
}

/// Where Windows differs: `MAX_PATH` is 260 characters unless long paths
/// are enabled, so the creation itself is the platform's answer. What
/// must not happen is a walk that trips on one and abandons the tree.
#[test]
fn a_path_over_260_characters_is_read_or_refused_cleanly() {
    let tree = Tree::new("long-path");
    tree.write("ordinary.yaml", &format!("id: {UUID}\n"));

    let mut deep = String::new();
    while deep.len() < 300 {
        deep.push_str("a-directory-with-a-long-name/");
    }
    let directory = tree.root.join(&deep);
    let target = directory.join("ids.yaml");
    if std::fs::create_dir_all(&directory).is_err()
        || std::fs::write(&target, format!("id: {UUID}\n")).is_err()
    {
        skipped(
            "a path over 260 characters",
            "this filesystem refused to create one",
        );
        return;
    }

    let run = execute(&[&tree.text()]);
    assert_answered(&run, "a path over 260 characters");
    let named: Vec<String> = reports(&run)
        .iter()
        .filter_map(|report| report["file"].as_str())
        .map(|file| file.rsplit(['/', '\\']).next().unwrap_or(file).to_string())
        .collect();
    assert!(
        named.iter().any(|file| file == "ordinary.yaml"),
        "the deep path took the rest of the tree with it: {named:?}\n{}",
        run.stderr
    );
    assert!(
        named.iter().any(|file| file == "ids.yaml"),
        "the deep file was created and then not walked: {named:?}"
    );
}

/// The three-way split `--strict` rests on, on one tree: a binary file
/// is not a report line, a text file that could not be decoded is, and
/// only the second fails `--strict`. A PNG in a repository must never
/// end an audit of everything beside it.
#[test]
fn a_binary_file_is_not_a_report_and_never_fails_strict() {
    let tree = Tree::new("binary");
    tree.write("ids.env", &format!("SERVICE_ID={UUID}\n"));
    tree.write_bytes("logo.png", &[0x89, 0x50, 0x4e, 0x47, 0x00, 0x1a, 0x0a]);

    let run = execute(&[&tree.text()]);
    assert_answered(&run, "a binary file");
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    let named: Vec<String> = reports(&run)
        .iter()
        .filter_map(|report| report["file"].as_str())
        .map(|file| file.rsplit(['/', '\\']).next().unwrap_or(file).to_string())
        .collect();
    assert_eq!(named, ["ids.env"], "the PNG produced a report line");
    assert!(
        run.stderr.contains("1 binary file skipped"),
        "{}",
        run.stderr
    );
    assert_eq!(
        execute(&["--strict", &tree.text()]).code,
        Some(0),
        "a binary file never fails --strict"
    );
}

/// Exit 2 is for a malformed *question*. A file the filesystem refused
/// is a fact about the tree; a flag nobody implemented is not.
#[test]
fn exit_two_is_for_a_malformed_question_and_nothing_else() {
    let tree = Tree::new("questions");
    tree.write("ids.yaml", &format!("id: {UUID}\n"));

    for malformed in [
        vec!["--nonsense", &tree.text()],
        vec!["--format"],
        vec!["--kind"],
        vec!["--kind", "guid", &tree.text()],
        vec!["--stdin", &tree.text()],
        vec!["/no/such/place-xyz"],
    ] {
        let run = execute(&malformed);
        assert_answered(&run, "a malformed question");
        assert_eq!(run.code, Some(2), "{malformed:?}\n{}", run.stderr);
        assert!(
            run.stdout.is_empty(),
            "{malformed:?} wrote to the protocol stream"
        );
    }
}
