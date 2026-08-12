//! A wall-clock ceiling on a generated corpus, and three linearity
//! checks beside it.
//!
//! A sibling was fifty times slower than the rest of the family for a
//! whole release and nothing noticed, because nothing measured it. The
//! ceiling here is deliberately loose — a shared runner is not a
//! benchmark rig — and exists to catch an order of magnitude, never a
//! percent.
//!
//! **Three linearity assertions, because this tool grows in three
//! directions and only the first is obvious:**
//!
//! - four times the *files* must not cost six times the clock;
//! - four times the *identifiers in one file* must not either — every
//!   candidate looks up the key path covering it, which is a binary
//!   search over the document's value spans and would be a scan if
//!   anybody "simplified" it;
//! - four times the identifiers **on one non-ASCII line** must not
//!   either. That is the one a sibling missed and this crate shipped:
//!   the column index re-counted UTF-16 units from the line start on
//!   every lookup, so a minified bundle carrying a single accented
//!   character went quadratic. Measured before the fix, on the release
//!   binary: 10,000 identifiers took 1.11 s and 20,000 took 4.03 s.
//!   After it, 0.03 s and 0.05 s.
//!
//! **The corpus is generated from a fixed seed, not checked in.** Five
//! hundred near-identical config files in git would be five hundred a
//! reviewer has to ignore, and the shape is what matters rather than the
//! bytes. The seed is fixed and printed, so a slow run is reproducible.
//!
//! Gated behind `IDS_LE_BUDGET` and run by CI on one platform with
//! `--test-threads=1`: a timing assertion measured against four other
//! tests on the same cores is noise. A skipped run says so by name.
//!
//! ## The measurement
//!
//! Taken 2026-08-12 on an Apple M-series laptop (macOS 25.4, `cargo test
//! --release`), warm page cache, best of two runs over the tree this
//! file generates:
//!
//! ```text
//! one tree  (500 files):   30 ms
//! four      (2000 files): 121 ms  — 3.9x, against a 6x allowance
//! ```
//!
//! The ceiling is 10x the first of those: 300 ms. It bounds an order of
//! magnitude, not a benchmark, and a GitHub runner is slower than this
//! laptop — if it lands near the line, re-measure there and say so in
//! this note rather than quietly raising the number.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BINARY: &str = env!("CARGO_BIN_EXE_ids-le");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 10x the local measurement recorded in the module note.
const CEILING: Duration = Duration::from_millis(300);

/// Four times the work, at most six times the clock. Anything quadratic
/// blows through this; ordinary variance does not.
const LINEARITY: f64 = 6.0;

/// Printed on every run, so a failure names the corpus that was slow.
const SEED: u64 = 20_260_812;

const FILES_PER_COPY: usize = 500;

fn enabled(name: &str) -> bool {
    if std::env::var_os("IDS_LE_BUDGET").is_some() {
        return true;
    }
    eprintln!("SKIPPED {name}: set IDS_LE_BUDGET to run it");
    false
}

/// A tiny xorshift, so the corpus is the same corpus on every machine
/// and every run without a dependency to get there.
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn pick<'a, T>(&mut self, from: &'a [T]) -> &'a T {
        let index = usize::try_from(self.next() % from.len() as u64).expect("an index");
        &from[index]
    }
}

struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(name: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ids-le-budget-{name}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a temporary directory");
        Self { root }
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The shapes a real checkout holds: config in every format this crate
/// keys, plus prose and source it can only read as text. `{U}` is a
/// UUID, `{L}` a ULID, `{O}` an ObjectId, `{S}` a Snowflake and `{H}` a
/// 32-hex run — one identifier of each kind, and one refusal, so the
/// measurement covers the formatting a refusal costs as well as a name.
const SHAPES: [(&str, &str); 8] = [
    (
        "json",
        "{{\"service\":{{\"id\":\"{U}\",\"digest\":\"{H}\"}},\"session\":{{\"ulid\":\"{L}\"}},\
         \"documents\":[{{\"_id\":\"{O}\"}}],\"discord\":{{\"channel_id\":{S}}}}}\n",
    ),
    (
        "yaml",
        "service:\n  id: {U}\n  digest: {H}\nsession:\n  ulid: {L}\ndocuments:\n  - _id: {O}\n\
         discord:\n  channel_id: {S}\n",
    ),
    (
        "toml",
        "digest = \"{H}\"\n\n[service]\nid = \"{U}\"\n\n[session]\nulid = \"{L}\"\n\n\
         [discord]\nchannel_id = {S}\n",
    ),
    (
        "ini",
        "digest = {H}\n\n[service]\nid = {U}\n\n[session]\nulid = {L}\n\n[discord]\n\
         channel_id = {S}\n",
    ),
    (
        "env",
        "SERVICE_ID={U}\nSESSION_ULID=\"{L}\"\nDIGEST={H}\nexport DISCORD_CHANNEL_ID={S}\n",
    ),
    (
        "csv",
        "name,service_id,channel_id\nalpha,{U},{S}\nbeta,{O},{S}\n",
    ),
    (
        "md",
        "# Release\n\nThe service ran under {U} and the session was {L}. The document was\n\
         {O}, and the digest {H}.\n",
    ),
    (
        "ts",
        "export const SERVICE = \"{U}\";\nexport const SESSION = \"{L}\";\n\
         // seen at {O}\nconst digest = \"{H}\";\n",
    ),
];

/// One document, with its identifiers varied per file so the corpus is
/// not one string the allocator has already seen.
fn document(seeded: &mut Seeded, index: usize) -> (&'static str, String) {
    let (extension, template) = *seeded.pick(&SHAPES);
    let body = template
        .replace("{U}", &format!("f47ac10b-58cc-4372-a567-{index:012x}"))
        .replace("{L}", "01KZSM9K00ABCDEFGH12345678")
        .replace("{O}", &format!("6a7bb780a1b2c3{index:010x}"))
        .replace("{S}", "1536886938009600000")
        .replace("{H}", &format!("5d41402abc4b2a76b9719{index:011x}"));
    let mut content = String::with_capacity(body.len() * 6);
    for _ in 0..6 {
        content.push_str(&body);
    }
    (extension, content)
}

/// Build one copy of the corpus: `FILES_PER_COPY` files under `prefix`,
/// spread over directories so the walk has something to walk.
fn populate(root: &Path, prefix: &str, seeded: &mut Seeded) {
    for index in 0..FILES_PER_COPY {
        let (extension, content) = document(seeded, index);
        let directory = root.join(format!("{prefix}/pkg{}", index % 25));
        std::fs::create_dir_all(&directory).expect("a directory");
        std::fs::write(directory.join(format!("file{index}.{extension}")), &content)
            .expect("a file");
    }
}

/// Time one scan, asserting it answered rather than refused.
fn scan(root: &Path) -> Duration {
    let started = Instant::now();
    let output = Command::new(BINARY)
        .arg(root)
        .output()
        .expect("the binary runs");
    let elapsed = started.elapsed();
    assert_eq!(
        output.status.code(),
        Some(0),
        "the scan did not finish: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.len() > 1000,
        "the scan reported almost nothing, so it measured almost nothing"
    );
    elapsed
}

/// Warm the page cache and the process, so the ceiling measures the scan
/// rather than the first read of a freshly written tree.
fn timed(root: &Path) -> Duration {
    let _ = scan(root);
    let first = scan(root);
    let second = scan(root);
    first.min(second)
}

/// A document of `count` identifiers, all on one line when `one_line`,
/// and carrying a single non-ASCII character when `unicode`.
///
/// That last flag is the whole point of the third check: one accented
/// character takes the column index off its arithmetic path, and before
/// the checkpoint index existed that made the whole document quadratic.
fn crowded(count: usize, one_line: bool, unicode: bool) -> String {
    let mut content = String::with_capacity(count * 50);
    content.push('{');
    if unicode {
        content.push_str("\"note\":\"caf\u{e9} \u{1f3af}\",");
    }
    for index in 1..=count {
        let _ = write!(
            content,
            "\"k{index}\":\"f47ac10b-58cc-4372-a567-{index:012x}\","
        );
        if !one_line {
            content.push('\n');
        }
    }
    content.push_str("\"end\":0}");
    content
}

/// Time a scan of one crowded document, written to its own tree.
fn timed_document(name: &str, content: &str) -> Duration {
    let tree = Tree::new(name);
    let file = tree.root.join("crowded.json");
    std::fs::write(&file, content).expect("a file");
    let _ = scan(&file);
    let first = scan(&file);
    let second = scan(&file);
    first.min(second)
}

fn ratio(one: Duration, four: Duration) -> f64 {
    four.as_secs_f64() / one.as_secs_f64().max(0.000_001)
}

#[test]
fn a_five_hundred_file_corpus_scans_inside_its_budget() {
    if !enabled("a_five_hundred_file_corpus_scans_inside_its_budget") {
        return;
    }
    let tree = Tree::new("ceiling");
    let mut seeded = Seeded(SEED);
    populate(&tree.root, "one", &mut seeded);

    let elapsed = timed(&tree.root);
    println!("budget: {FILES_PER_COPY} files scanned in {elapsed:?} (seed {SEED})");
    assert!(
        elapsed <= CEILING,
        "seed {SEED}: {FILES_PER_COPY} files took {elapsed:?}, over the {CEILING:?} ceiling — \
         10x the recorded local measurement, so this is an order of magnitude, not variance"
    );
}

/// The quadratic class across files: four times the tree must not take
/// more than six times as long.
#[test]
fn four_times_the_files_is_not_six_times_the_clock() {
    if !enabled("four_times_the_files_is_not_six_times_the_clock") {
        return;
    }
    let small = Tree::new("linear-one");
    let large = Tree::new("linear-four");
    let mut seeded = Seeded(SEED);
    populate(&small.root, "one", &mut seeded);
    for copy in 0..4 {
        let mut same = Seeded(SEED);
        populate(&large.root, &format!("copy{copy}"), &mut same);
    }

    let one = timed(&small.root);
    let four = timed(&large.root);
    let measured = ratio(one, four);
    println!("budget: one tree {one:?}, four trees {four:?}, ratio {measured:.2}x (seed {SEED})");
    assert!(
        measured <= LINEARITY,
        "seed {SEED}: four times the tree took {measured:.2}x as long ({one:?} then {four:?}), \
         over the {LINEARITY}x allowance — that is the shape of something quadratic"
    );
}

/// The quadratic class inside one file. Every candidate looks up the key
/// path covering it, and a binary search is what keeps that from being a
/// scan of every value span in the document.
#[test]
fn four_times_the_identifiers_in_one_file_is_not_six_times_the_clock() {
    if !enabled("four_times_the_identifiers_in_one_file_is_not_six_times_the_clock") {
        return;
    }
    let one = timed_document("crowded-one", &crowded(20_000, false, false));
    let four = timed_document("crowded-four", &crowded(80_000, false, false));
    let measured = ratio(one, four);
    println!("budget: 20,000 identifiers {one:?}, 80,000 {four:?}, ratio {measured:.2}x");
    assert!(
        measured <= LINEARITY,
        "four times the identifiers in one file took {measured:.2}x as long \
         ({one:?} then {four:?}), over the {LINEARITY}x allowance"
    );
}

/// **The one this crate actually shipped.** A minified bundle is one
/// line; a single accented character anywhere in it takes every column
/// lookup off the arithmetic path; and before `extract/position.rs` grew
/// its checkpoint index, each lookup re-counted UTF-16 code units from
/// the start of that line. Twenty thousand identifiers took four
/// seconds.
///
/// The ASCII copy is measured beside it, because a fix that made the
/// unicode path linear by making the ASCII one slow would be no fix at
/// all.
#[test]
fn four_times_the_identifiers_on_one_unicode_line_is_not_six_times_the_clock() {
    if !enabled("four_times_the_identifiers_on_one_unicode_line_is_not_six_times_the_clock") {
        return;
    }
    let ascii_one = timed_document("line-ascii-one", &crowded(10_000, true, false));
    let ascii_four = timed_document("line-ascii-four", &crowded(40_000, true, false));
    let unicode_one = timed_document("line-unicode-one", &crowded(10_000, true, true));
    let unicode_four = timed_document("line-unicode-four", &crowded(40_000, true, true));

    let ascii = ratio(ascii_one, ascii_four);
    let unicode = ratio(unicode_one, unicode_four);
    println!(
        "budget: one line, ascii {ascii_one:?} then {ascii_four:?} ({ascii:.2}x); \
         unicode {unicode_one:?} then {unicode_four:?} ({unicode:.2}x)"
    );
    assert!(
        unicode <= LINEARITY,
        "four times the identifiers on one non-ASCII line took {unicode:.2}x as long \
         ({unicode_one:?} then {unicode_four:?}) — the column index is counting from the \
         line start again"
    );
    assert!(
        ascii <= LINEARITY,
        "four times the identifiers on one ASCII line took {ascii:.2}x as long \
         ({ascii_one:?} then {ascii_four:?})"
    );
    // And the two paths must stay within sight of each other: a unicode
    // document may cost more per lookup, never an order of magnitude
    // more.
    let gap = ratio(ascii_four, unicode_four);
    assert!(
        gap <= LINEARITY,
        "the same document with one accented character in it took {gap:.2}x as long \
         ({ascii_four:?} then {unicode_four:?})"
    );
}
