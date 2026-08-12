//! A standing net over the classifier, fed runs nobody would write by
//! hand.
//!
//! Time-boxed, not run to convergence: the point is that a panic, a
//! hang, a slice off a character boundary or a row that names a kind
//! when two fit has somewhere to be caught, not that the input space is
//! proved. Sixty seconds in CI, a second locally, from a seed that is
//! printed on every run — a fuzz failure nobody can reproduce is a fuzz
//! failure nobody fixes.
//!
//! **The generator aims at the boundaries, not at random bytes.** Every
//! decision this crate makes turns on a length, an alphabet, a nibble or
//! a window, so the cases are built to sit one character either side of
//! each:
//!
//! - hex runs at every length near 24 (ObjectId) and 32 (unhyphenated
//!   UUID / MD5);
//! - base62 runs near 21 (NanoID), with and without the `-`/`_` that is
//!   the only evidence separating base64url from base62;
//! - Crockford base32 carrying the four letters the alphabet excludes —
//!   `I`, `L`, `O`, `U`;
//! - ULIDs whose leading character encodes above `7`, which a 48-bit
//!   timestamp cannot reach;
//! - UUIDs over **every** version and variant nibble combination, all
//!   256 of them;
//! - integers spanning the Snowflake range, under keys that name a
//!   platform and keys that do not.
//!
//! Four things are asserted, and the last is the one this crate exists
//! for: it never panics, it always answers inside its deadline, every
//! row is well formed — and **a run that fits two schemes comes back
//! refused, never named.**

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::File;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BINARY: &str = env!("CARGO_BIN_EXE_ids-le");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// One batch may not take longer than this. A classifier that went
/// exponential on some length would otherwise be a CI job that never
/// ends.
const BATCH_LIMIT: Duration = Duration::from_secs(30);

/// Runs per document. Large enough that the cost is the classifier
/// rather than the process, small enough that a failure names a
/// document a person can still read.
const BATCH: usize = 400;

/// Seconds of fuzzing. CI passes 60; a bare `cargo test` runs one, so
/// the net is present on every push without owning the run.
fn budget() -> Duration {
    Duration::from_secs(
        std::env::var("IDS_LE_FUZZ_SECONDS")
            .ok()
            .and_then(|raw| raw.parse::<u64>().ok())
            .unwrap_or(1),
    )
}

/// Printed on every run, failing or not.
fn seed() -> u64 {
    std::env::var("IDS_LE_FUZZ_SEED")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(0x1d5e_0b1d_2026_0812)
}

/// xorshift64*, four lines and identical on every platform. A fuzz run
/// needs to be reproducible, not statistically excellent.
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.0 = state;
        state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, limit: usize) -> usize {
        let limit = u64::try_from(limit).unwrap_or(1).max(1);
        usize::try_from(self.next() % limit).unwrap_or(0)
    }

    fn between(&mut self, low: i64, high: i64) -> i64 {
        let span = u64::try_from(high - low).unwrap_or(1).max(1);
        low + i64::try_from(self.next() % span).unwrap_or(0)
    }

    fn pick(&mut self, from: &[u8]) -> char {
        char::from(from[self.below(from.len())])
    }
}

/// What the report must say about one generated run.
///
/// Nothing here pins a kind the generator cannot be sure of — a random
/// 24-hex run is an ObjectId or an ambiguity depending on four bytes
/// rolled at random, and asserting either would be asserting the
/// implementation back at itself. What is pinned is what the design
/// promises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// Only the floor: a row or no row, and if there is one it is well
    /// formed.
    Anything,
    /// No kind claims this shape, so there must be no row at all.
    Ignored,
    /// There must be a row. Whether it is named or refused depends on
    /// bytes the generator rolled.
    Row,
    /// Two schemes fit by construction, so naming one would be a guess.
    Refused,
    /// Refused, with this reason and no other.
    Reason(&'static str),
    /// Named, as this kind. Only where the document itself settles it —
    /// a base64url character, or a key naming the scheme.
    Named(&'static str),
}

/// One generated case: the key it sits under, the run itself, and what
/// the report must say.
struct Case {
    key: String,
    token: String,
    expect: Expect,
    /// The generator that produced it, so a failure names the family
    /// rather than the seed.
    family: &'static str,
}

const HEX: &[u8] = b"0123456789abcdef";
const HEX_LETTERS: &[u8] = b"abcdef";
const DIGITS: &[u8] = b"0123456789";
const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
/// Crockford base32: the digits, then the letters with `I`, `L`, `O` and
/// `U` removed.
const CROCKFORD: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// The four letters Crockford leaves out, and the whole reason a ULID
/// can be malformed at all.
const EXCLUDED: &[u8] = b"ILOU";

/// 2010-11-04T01:42:54.657Z and 2015-01-01T00:00:00.000Z, the two
/// epochs `snowflake.rs` knows. Restated here rather than imported:
/// these tests drive the binary, and a constant shared with the code
/// under test would move with it.
const TWITTER_EPOCH_MS: i64 = 1_288_834_974_657;
const DISCORD_EPOCH_MS: i64 = 1_420_070_400_000;
/// 22 bits of worker id and sequence sit below the timestamp.
const TIMESTAMP_SHIFT: u32 = 22;

fn run_of(seeded: &mut Seeded, alphabet: &[u8], length: usize) -> String {
    (0..length).map(|_| seeded.pick(alphabet)).collect()
}

/// Hex runs at every length the router cares about, and one either side
/// of each.
///
/// 23/25 and 31/33 are the near misses `policy.rs` refuses as malformed
/// — but only when a letter is present, which is the rule that keeps a
/// 19-digit Snowflake out of it, so a letter is forced there. 26 hex
/// characters are a ULID's length and every hex letter is in Crockford's
/// alphabet, which is why lowercase hex of that length is a refusal
/// rather than nothing.
///
/// Half the runs sit under a key ending in `id` and half do not, because
/// at 24 characters that is the whole question: an ObjectId has no
/// structure to check, so a run with no field naming it an identifier is
/// refused however good its timestamp looks.
fn hex_case(seeded: &mut Seeded, index: usize) -> Case {
    let length = *[22usize, 23, 24, 25, 26, 30, 31, 32, 33, 34]
        .get(seeded.below(10))
        .expect("a length");
    let mut token = run_of(seeded, HEX, length);
    // A letter and a digit, both away from the front: the near-miss rule
    // wants a letter, a 26-character run needs both to be
    // identifier-shaped, and the **leading** four bytes are left random
    // because for a 24-character run they are the other half of the
    // question.
    token.replace_range(1..2, &seeded.pick(HEX_LETTERS).to_string());
    token.replace_range(2..3, &seeded.pick(DIGITS).to_string());

    let named_field = seeded.below(2) == 0;
    let expect = match (length, named_field) {
        // Named the field an identifier, so the leading four bytes get
        // to decide: an ObjectId if they are a plausible time, an
        // ambiguity if they are not, up to the bytes just rolled.
        (24, true) => Expect::Row,
        // The two lengths where a hash fits the shape exactly. 32 hex
        // digits are an unhyphenated UUID and an MD5 digest; 24 with
        // nothing naming the field an identifier are an ObjectId and a
        // truncated SHA-1. Neither can be settled from the run.
        (32, _) | (24, false) => Expect::Reason("ambiguous_kind"),
        (23 | 25 | 31 | 33, _) => Expect::Reason("malformed"),
        // A ULID's length, in an alphabet Crockford accepts, spelled in
        // lowercase — which is never the canonical form, so it is a
        // refusal whichever rule catches it first.
        (26, _) => Expect::Refused,
        _ => Expect::Ignored,
    };
    let key = if named_field {
        format!("k{index}_id")
    } else {
        format!("k{index}")
    };
    Case {
        key,
        token,
        expect,
        family: "hex near 24 and 32",
    }
}

/// base62 and base64url runs near 21. The gate in `nanoid.rs` wants a
/// digit and both cases, so all three are forced — without them the run
/// is a slug and is ignored, which would test the gate rather than the
/// rule.
fn nanoid_case(seeded: &mut Seeded, index: usize) -> Case {
    let length = *[20usize, 21, 22].get(seeded.below(3)).expect("a length");
    let mut token = run_of(seeded, BASE62, length);
    token.replace_range(0..1, &seeded.pick(DIGITS).to_string());
    token.replace_range(1..2, &seeded.pick(UPPER).to_string());
    token.replace_range(2..3, &seeded.pick(LOWER).to_string());

    let url_safe = seeded.below(2) == 0;
    if url_safe {
        let at = 3 + seeded.below(length - 4);
        token.replace_range(at..=at, if seeded.below(2) == 0 { "-" } else { "_" });
    }
    let expect = match (length, url_safe) {
        // The one character that separates base64url from base62 is the
        // only evidence a NanoID has, and it is present.
        (21, true) => Expect::Named("nanoid"),
        // 21 base62 characters are a NanoID, a short token and a
        // truncated hash in equal measure.
        (21, false) => Expect::Reason("ambiguous_kind"),
        _ => Expect::Ignored,
    };
    Case {
        key: format!("k{index}"),
        token,
        expect,
        family: "base62 near 21",
    }
}

/// 26 Crockford characters carrying one of the four letters the alphabet
/// excludes. The exclusions are the only structural check a ULID has, so
/// they are the only thing `malformed` can mean there.
fn crockford_case(seeded: &mut Seeded, index: usize) -> Case {
    let mut token = run_of(seeded, CROCKFORD, 26);
    token.replace_range(0..1, &seeded.pick(b"01234567").to_string());
    token.replace_range(1..2, &seeded.pick(DIGITS).to_string());
    token.replace_range(2..3, &seeded.pick(b"ABCDEFGH").to_string());
    let at = 3 + seeded.below(23);
    token.replace_range(at..=at, &seeded.pick(EXCLUDED).to_string());
    Case {
        key: format!("k{index}"),
        token,
        expect: Expect::Reason("malformed"),
        family: "crockford with an excluded letter",
    }
}

/// A ULID whose leading character encodes above `7`. Ten base32
/// characters hold 50 bits and the timestamp is 48, so the first
/// character can only use three of its five — a value no generator can
/// produce, and one no amount of clock plausibility can rescue.
fn ulid_overflow_case(seeded: &mut Seeded, index: usize) -> Case {
    let mut token = run_of(seeded, CROCKFORD, 26);
    // Index 8 upwards in the alphabet: '8', '9', then the letters.
    token.replace_range(0..1, &seeded.pick(&CROCKFORD[8..]).to_string());
    token.replace_range(1..2, &seeded.pick(DIGITS).to_string());
    token.replace_range(2..3, &seeded.pick(b"ABCDEFGH").to_string());
    Case {
        key: format!("k{index}"),
        token,
        expect: Expect::Reason("malformed"),
        family: "ulid leading character above 7",
    }
}

/// A canonically hyphenated UUID with a chosen version and variant
/// nibble. Every combination of the two is reachable, and most of them
/// are refusals — which is the point: those two nibbles are a
/// self-description, and a self-description can disagree with the bytes
/// around it.
fn uuid_of(seeded: &mut Seeded, version: usize, variant: usize) -> String {
    let mut hex = run_of(seeded, HEX, 32);
    hex.replace_range(12..13, &char::from(HEX[version]).to_string());
    hex.replace_range(16..17, &char::from(HEX[variant]).to_string());
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// What a version and variant nibble pair must produce, whatever the
/// bytes around them.
const fn uuid_expectation(version: usize, variant: usize) -> Expect {
    match (version, variant) {
        // Outside the RFC variant the version nibble is not a version,
        // so the row is malformed and reports the variant instead; and
        // inside it, RFC 9562 defines versions 1 through 8 and nothing
        // else.
        (_, 0..=7 | 12..=15) | (0 | 9..=15, _) => Expect::Reason("malformed"),
        // A version and variant that hold together: named, unless a
        // decoded timestamp lands outside the window or a v4 turns out
        // not to look random. Both are refusals with their own reasons,
        // and both depend on bytes rolled at random.
        _ => Expect::Row,
    }
}

fn uuid_case(seeded: &mut Seeded, index: usize) -> Case {
    let version = seeded.below(16);
    let variant = seeded.below(16);
    Case {
        key: format!("k{index}"),
        token: uuid_of(seeded, version, variant),
        expect: uuid_expectation(version, variant),
        family: "uuid version and variant nibbles",
    }
}

/// An integer across the Snowflake range, under a key that names a
/// platform or one that does not.
///
/// One in three is built so that **both** epochs decode it to a
/// plausible instant — 2020 through 2026 under Discord's, which puts
/// Twitter's four years earlier and leaves both inside the window
/// whatever day this runs. That is the case the boundary is about: with
/// two epochs fitting and no platform named, the row must come back
/// refused rather than given one.
fn snowflake_case(seeded: &mut Seeded, index: usize) -> Case {
    let both_epochs_fit = seeded.below(3) == 0;
    if both_epochs_fit {
        let discord_ms = seeded.between(1_577_836_800_000, 1_767_225_600_000);
        let shifted = discord_ms - DISCORD_EPOCH_MS;
        let value =
            (u64::try_from(shifted).unwrap_or(0) << TIMESTAMP_SHIFT) | (seeded.next() % (1 << 22));
        return Case {
            key: format!("k{index}_user_id"),
            token: value.to_string(),
            expect: Expect::Refused,
            family: "a snowflake both epochs explain",
        };
    }

    let digits = 16 + seeded.below(5); // 16 to 20: two of them out of range
    let mut token = String::with_capacity(digits);
    token.push(seeded.pick(b"123456789"));
    for _ in 1..digits {
        token.push(seeded.pick(DIGITS));
    }

    let key = match seeded.below(3) {
        0 => format!("k{index}_channel_id"),
        1 => format!("k{index}_tweet_id"),
        _ => format!("k{index}_user_id"),
    };
    let expect = match digits {
        17..=19 => Expect::Row,
        // Outside the router's window a bare integer is not a candidate
        // at all — which is the whole reason a Snowflake needs a key.
        _ => Expect::Ignored,
    };
    Case {
        key,
        token,
        expect,
        family: "integers across the snowflake range",
    }
}

fn case(seeded: &mut Seeded, index: usize) -> Case {
    match seeded.below(6) {
        0 => hex_case(seeded, index),
        1 => nanoid_case(seeded, index),
        2 => crockford_case(seeded, index),
        3 => ulid_overflow_case(seeded, index),
        4 => uuid_case(seeded, index),
        _ => snowflake_case(seeded, index),
    }
}

/// One document through the binary, and the rows it produced.
fn scan(label: &str, cases: &[Case]) -> Vec<serde_json::Value> {
    let mut document = String::new();
    for one in cases {
        let _ = writeln!(document, "{}: {}", one.key, one.token);
    }

    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let capture = std::env::temp_dir().join(format!("ids-le-fuzz-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&capture).expect("a capture directory");
    let out = capture.join("stdout");

    let mut child = Command::new(BINARY)
        .args(["--stdin", "--format", "yaml"])
        .stdin(Stdio::piped())
        .stdout(File::create(&out).expect("a stdout file"))
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary runs");
    {
        use std::io::Write as _;
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(document.as_bytes())
            .expect("the child is still reading");
    }
    drop(child.stdin.take());

    let started = Instant::now();
    let status = loop {
        match child.try_wait().expect("the child can be waited on") {
            Some(status) => break status,
            None if started.elapsed() >= BATCH_LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{label}: no answer in {BATCH_LIMIT:?} — the classifier is not terminating");
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };

    let code = status
        .code()
        .unwrap_or_else(|| panic!("{label}: the process died on a signal\n{document}"));
    assert!(
        code == 0 || code == 1,
        "{label}: a well-formed document exited {code}\n{document}"
    );

    let stdout = String::from_utf8_lossy(&std::fs::read(&out).unwrap_or_default()).into_owned();
    let _ = std::fs::remove_dir_all(&capture);

    let report: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|error| panic!("{label}: stdout is not JSON ({error}) — {stdout}"));
    let rows = report["ids"].as_array().expect("rows").clone();

    // The summary describes the rows it reported and nothing else.
    let named =
        u64::try_from(rows.iter().filter(|row| row["valid"] == true).count()).expect("a count");
    let total = u64::try_from(rows.len()).expect("a count");
    assert_eq!(
        report["summary"]["ids"].as_u64(),
        Some(named),
        "{label}: the summary and the rows disagree"
    );
    assert_eq!(
        report["summary"]["refused"].as_u64(),
        Some(total - named),
        "{label}: the summary and the rows disagree"
    );
    rows
}

const REASONS: [&str; 5] = [
    "ambiguous_kind",
    "malformed",
    "nil_or_max",
    "version_claim_mismatch",
    "timestamp_implausible",
];
const KINDS: [&str; 5] = ["uuid", "ulid", "nanoid", "objectid", "snowflake"];

/// The floor every row shares, whatever produced it.
fn check_row(label: &str, row: &serde_json::Value) {
    let value = row["value"].as_str().expect("a value");
    assert!(
        row["line"].as_u64().is_some_and(|line| line >= 1),
        "{label}: {value} has no line"
    );
    assert!(
        row["column"].as_u64().is_some_and(|column| column >= 1),
        "{label}: {value} has no column"
    );

    let valid = row["valid"].as_bool().expect("a verdict");
    if valid {
        let kind = row["kind"].as_str().unwrap_or_default();
        assert!(KINDS.contains(&kind), "{label}: {value} was named {kind:?}");
        assert!(
            row.get("refused").is_none(),
            "{label}: {value} was named and refused at once"
        );
        // A version is only a version under the RFC variant; in any
        // other layout those four bits belong to a different field, so
        // reporting one from them would be inventing it.
        assert!(
            row.get("version").is_none() || row["variant"] == "rfc4122",
            "{label}: {value} reports a version under variant {}",
            row["variant"]
        );
        return;
    }

    let reason = row["refused"].as_str().unwrap_or_default();
    assert!(
        REASONS.contains(&reason),
        "{label}: {value} was refused as {reason:?}"
    );
    assert!(
        row["detail"].as_str().is_some_and(|text| text.len() > 20),
        "{label}: {value} was refused without saying why"
    );
    assert!(
        row["kind"].is_null() || KINDS.contains(&row["kind"].as_str().unwrap_or_default()),
        "{label}: {value} was refused as kind {}",
        row["kind"]
    );
}

/// Every case against the row it produced — or against the absence of
/// one, where no kind claims the shape.
fn check_batch(label: &str, cases: &[Case], rows: &[serde_json::Value]) {
    let mut by_key: BTreeMap<&str, &serde_json::Value> = BTreeMap::new();
    for row in rows {
        check_row(label, row);
        let key = row["key"].as_str().unwrap_or_default();
        assert!(
            by_key.insert(key, row).is_none(),
            "{label}: two rows under {key}"
        );
    }

    for one in cases {
        let where_it_is = format!("{label} [{}] {} = {}", one.family, one.key, one.token);
        let Some(row) = by_key.get(one.key.as_str()) else {
            assert!(
                matches!(one.expect, Expect::Ignored | Expect::Anything),
                "{where_it_is}: no row was reported"
            );
            continue;
        };
        assert_eq!(row["value"], one.token, "{where_it_is}: the value moved");

        match one.expect {
            Expect::Ignored => panic!("{where_it_is}: a shape no kind claims produced {row}"),
            // The floor, already asserted by `check_row`.
            Expect::Anything | Expect::Row => {}
            Expect::Refused => assert_eq!(
                row["valid"], false,
                "{where_it_is}: two schemes fit and one was named — {row}"
            ),
            Expect::Reason(reason) => assert_eq!(
                row["refused"], reason,
                "{where_it_is}: expected {reason} — {row}"
            ),
            Expect::Named(kind) => {
                assert_eq!(row["valid"], true, "{where_it_is}: refused — {row}");
                assert_eq!(row["kind"], kind, "{where_it_is}: named the wrong kind");
            }
        }
    }
}

#[test]
fn the_classifier_survives_runs_nobody_would_write() {
    let seed = seed();
    let budget = budget();
    eprintln!("fuzz: seed {seed:#x}, budget {budget:?}");

    let mut seeded = Seeded(seed);
    let mut documents = 0usize;
    let mut runs = 0usize;
    let deadline = Instant::now() + budget;

    while Instant::now() < deadline {
        let cases: Vec<Case> = (0..BATCH).map(|index| case(&mut seeded, index)).collect();
        let label = format!("seed {seed:#x} document {documents}");
        let rows = scan(&label, &cases);
        check_batch(&label, &cases, &rows);
        documents += 1;
        runs += cases.len();
    }

    eprintln!("fuzz: {runs} runs over {documents} documents, seed {seed:#x}");
    assert!(documents > 0, "the fuzzer ran no documents");
}

/// **All 256 version-and-variant combinations**, by construction rather
/// than by luck. The random path reaches them eventually; this one
/// reaches them on every run, and a failure names the nibbles rather
/// than the seed.
#[test]
fn every_uuid_version_and_variant_nibble_combination_is_answered() {
    let mut seeded = Seeded(seed());
    let mut cases = Vec::with_capacity(256);
    for version in 0..16usize {
        for variant in 0..16usize {
            cases.push(Case {
                key: format!("v{version}r{variant}"),
                token: uuid_of(&mut seeded, version, variant),
                expect: uuid_expectation(version, variant),
                family: "uuid nibble matrix",
            });
        }
    }

    let label = "uuid nibble matrix";
    let rows = scan(label, &cases);
    check_batch(label, &cases, &rows);
    assert_eq!(rows.len(), 256, "a combination produced no row at all");

    // Versions 1 through 8 under the four RFC variant nibbles are the
    // only rows that may carry a version — and every one of them does,
    // named or refused.
    let with_version = rows
        .iter()
        .filter(|row| row.get("version").is_some())
        .count();
    assert_eq!(
        with_version,
        8 * 4,
        "a version was reported where the nibbles do not define one, or dropped where they do"
    );
}

/// The shapes at every boundary, by name and outside the random path, so
/// a failure says what broke rather than which seed found it.
#[test]
fn the_shapes_at_every_boundary_are_answered() {
    let cases: Vec<Case> = [
        // The nil and max UUIDs: structurally perfect, and RFC 9562
        // says both name nothing.
        (
            "k1",
            "00000000-0000-0000-0000-000000000000",
            Expect::Reason("nil_or_max"),
        ),
        (
            "k2",
            "ffffffff-ffff-ffff-ffff-ffffffffffff",
            Expect::Reason("nil_or_max"),
        ),
        // A v4 that is plainly not 122 random bits.
        (
            "k3",
            "00000000-0000-4000-8000-000000000000",
            Expect::Reason("version_claim_mismatch"),
        ),
        // A v7 in the year 6429.
        (
            "k4",
            "7fffffff-ffff-7abc-8def-0123456789ab",
            Expect::Reason("timestamp_implausible"),
        ),
        // A ULID at the top of its 48-bit field, and one at the bottom:
        // the decode succeeds and lands ten thousand years out, or on
        // the Unix epoch twenty years before the window opens.
        (
            "k5",
            "7ZZZZZZZZZABCDEFGH12345678",
            Expect::Reason("timestamp_implausible"),
        ),
        (
            "k6",
            "0000000000ABCDEFGH12345678",
            Expect::Reason("timestamp_implausible"),
        ),
        // 32 hex digits, the one genuine cross-kind collision.
        (
            "k7",
            "5d41402abc4b2a76b9719d911017c592",
            Expect::Reason("ambiguous_kind"),
        ),
        // 24 hex digits whose leading four bytes are 1970 and 2106,
        // under a key that does name an identifier: the field is right
        // and the time is not, so still no kind.
        ("k8_id", "00000001a1b2c3d4e5f60718", Expect::Refused),
        ("k9_id", "ffffffffa1b2c3d4e5f60718", Expect::Refused),
        // The same 24 characters twice: named where the document calls
        // the field an identifier, refused where it does not. An
        // ObjectId's whole specification is 24 hex digits, so the run
        // can never settle this alone.
        (
            "k8b_id",
            "6a7bb780a1b2c3d4e5f60718",
            Expect::Named("objectid"),
        ),
        (
            "k9b",
            "6a7bb780a1b2c3d4e5f60718",
            Expect::Reason("ambiguous_kind"),
        ),
        // A candidate longer than any kind, and one shorter than all of
        // them: no row either way.
        (
            "k10",
            "user_550e8400-e29b-41d4-a716-446655440000",
            Expect::Ignored,
        ),
        ("k11", "0123456789abcdef", Expect::Ignored),
        // A 19-digit run under a key that names no id — the boundary
        // the whole Snowflake module is built around.
        ("k12", "1536886938009600000", Expect::Ignored),
        // The same integer under a key that names one, with no platform
        // named: both epochs fit, so neither is chosen.
        ("k13_user_id", "1536886938009600000", Expect::Refused),
        // And under one that names the platform.
        (
            "k14_channel_id",
            "1536886938009600000",
            Expect::Named("snowflake"),
        ),
        // A run of hyphens at a UUID's length, one hyphenated in the
        // wrong places, and one wearing underscores instead: none of
        // them is a misspelled UUID, and none may be read as one.
        (
            "k15",
            "------------------------------------",
            Expect::Ignored,
        ),
        (
            "k16",
            "f47ac10b58cc-4372-a567-0e02b2c3d4791",
            Expect::Ignored,
        ),
        (
            "k17",
            "f47ac10b_58cc_4372_a567_0e02b2c3d479",
            Expect::Ignored,
        ),
    ]
    .into_iter()
    .map(|(key, token, expect)| Case {
        key: key.to_string(),
        token: token.to_string(),
        expect,
        family: "boundary",
    })
    .collect();

    let label = "boundaries";
    let rows = scan(label, &cases);
    check_batch(label, &cases, &rows);
}

/// Every length the candidate scanner accepts, from each alphabet, plus
/// the lengths either side of its window. None of these is an
/// identifier and almost none is a row — what matters is that a short
/// run never reaches an index that is not there.
#[test]
fn every_candidate_length_is_survivable() {
    let mut seeded = Seeded(seed());
    let mut cases = Vec::new();
    for length in 1..=40usize {
        for (family, alphabet) in [
            ("hex", HEX),
            ("digits", DIGITS),
            ("letters", LOWER),
            ("crockford", CROCKFORD),
            ("base62", BASE62),
        ] {
            cases.push(Case {
                key: format!("k{}", cases.len()),
                token: run_of(&mut seeded, alphabet, length),
                expect: Expect::Anything,
                family,
            });
        }
    }

    let label = "every candidate length";
    let rows = scan(label, &cases);
    check_batch(label, &cases, &rows);
}

/// The two epochs, restated: a Snowflake sitting where only one of them
/// gives a plausible instant **is** named, and the row says which
/// instant. So the refusal above reads as a decision rather than an
/// inability.
///
/// **The token is built from the clock, and that is the point rather than
/// a compromise.** Discord's epoch is 4.16 years after Twitter's and the
/// plausibility window ends a year past now, so the band where exactly
/// one epoch fits is 4.16 years wide and slides with the calendar. A
/// pinned token sits in it only for a while: the one that used to be here
/// decoded to 2025 under Twitter and 2029 under Discord, and would have
/// started failing in March 2028 when the window's upper edge reached the
/// second of those. A test with a shelf life is a test that will one day
/// be deleted by whoever is unlucky enough to be on shift.
///
/// Built from `now`, Twitter's decode is this instant and Discord's is
/// four years out, whenever this runs.
///
/// The assertion is not a literal timestamp — there is none to write —
/// but something stronger: the same integer under a key that names
/// Twitter outright must decode identically. If the unkeyed row had
/// picked Discord's epoch, the two would differ by 4.16 years.
#[test]
fn a_snowflake_only_one_epoch_can_explain_is_named() {
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_millis(),
    )
    .expect("an instant that fits i64");
    let shifted = u64::try_from(now - TWITTER_EPOCH_MS).expect("a clock after 2010");
    let token = (shifted << TIMESTAMP_SHIFT).to_string();
    assert!(
        (17..=19).contains(&token.len()),
        "the generated snowflake left the router's window: {token}"
    );

    let cases = vec![
        Case {
            key: "session_id".to_string(),
            token: token.clone(),
            expect: Expect::Named("snowflake"),
            family: "one epoch fits",
        },
        Case {
            key: "tweet_id".to_string(),
            token,
            expect: Expect::Named("snowflake"),
            family: "twitter named outright",
        },
    ];

    let label = "one epoch fits";
    let rows = scan(label, &cases);
    check_batch(label, &cases, &rows);

    let decoded = |key: &str| {
        rows.iter()
            .find(|row| row["key"] == key)
            .unwrap_or_else(|| panic!("no row under {key}"))["timestamp"]
            .clone()
    };
    assert_eq!(
        decoded("session_id"),
        decoded("tweet_id"),
        "the epoch the clock chose is not the one that fits"
    );
}
