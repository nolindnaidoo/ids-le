//! Does this crate open what it claims to open, and name what it claims
//! to name?
//!
//! Every kind, every refusal reason, every UUID version and variant, and
//! every format reader, driven from **real fixture documents** through
//! the built binary. A kind in the enum that no document produces is a
//! kind nobody is checking; a format offered in the tool schema that no
//! document exercises is the same thing one layer down.
//!
//! Two of the lists are read out of the source rather than typed here,
//! because a matrix that names its own expectations is a matrix that
//! cannot notice a sixth kind arriving: `Kind`, `Reason` and `Variant`
//! come from `src/extract/policy.rs`, and the offered formats from
//! `src/extract/format.rs`. Adding a variant to either without a fixture
//! to produce it fails this file.
//!
//! `fixtures/documents/versions.json` exists for this suite: the README
//! and SPEC.md both say "all versions", and until it was written the
//! corpus held v1, v4, v6 and v7 — half the claim. It carries one UUID
//! of every version RFC 9562 defines and one of every variant, so "all
//! versions" is a sentence with a document behind it.
//!
//! **The marker line is load-bearing.** `cargo test <filter>` exits 0
//! when the filter matches nothing, so a renamed or deleted test here
//! would leave a green CI job that ran no assertions at all. The last
//! thing this suite prints is `coverage-matrix: complete`, and the CI
//! job greps for it.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

const BINARY: &str = env!("CARGO_BIN_EXE_ids-le");
const POLICY: &str = include_str!("../src/extract/policy.rs");
const FORMAT: &str = include_str!("../src/extract/format.rs");

/// The versions RFC 9562 defines, which is what `uuid.rs` accepts and
/// what SPEC.md and the README both claim.
const VERSIONS: [i64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

fn documents() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/documents")
}

/// Every report line from a scan of the fixture directory.
fn scan(path: &PathBuf) -> Vec<serde_json::Value> {
    let output = Command::new(BINARY)
        .arg(path)
        .output()
        .expect("the binary runs");
    assert!(
        matches!(output.status.code(), Some(0 | 1)),
        "the scan did not finish: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("stdout carries only JSON"))
        .collect()
}

fn rows(reports: &[serde_json::Value]) -> Vec<serde_json::Value> {
    reports
        .iter()
        .flat_map(|report| report["ids"].as_array().expect("rows").clone())
        .collect()
}

/// The variant names of one enum, read out of the source it is declared
/// in. A list typed into a test drifts; a list read from the code
/// cannot.
fn variants_of(source: &str, name: &str) -> Vec<String> {
    let declaration = format!("enum {name} {{");
    let start = source
        .find(&declaration)
        .unwrap_or_else(|| panic!("{name} is not declared where this test looks for it"));
    let body = &source[start + declaration.len()..];
    let end = body
        .find("\n}")
        .unwrap_or_else(|| panic!("{name} has no closing brace"));

    body[..end]
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//") && !line.starts_with('#'))
        .map(|line| line.trim_end_matches(',').to_string())
        .collect()
}

/// `AmbiguousKind` as serde writes it under `rename_all = "snake_case"`.
fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            out.push('_');
        }
        out.extend(character.to_lowercase());
    }
    out
}

/// The formats the tool schema offers, read out of `format.rs`.
fn offered_formats() -> Vec<String> {
    let start = FORMAT
        .find("SUPPORTED_FORMATS")
        .expect("the offered list is declared");
    let list = &FORMAT[start..];
    // The first `=` after the name is the assignment; the brackets
    // before it belong to the type.
    let assigned = list.find('=').expect("an assignment");
    let open = list[assigned..].find('[').expect("a list") + assigned;
    let close = list[open..].find(']').expect("a closing bracket") + open;
    list[open + 1..close]
        .split(',')
        .map(|entry| entry.trim().trim_matches('"').to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// **The matrix itself.** One scan of the fixture directory, and every
/// enum the report can carry accounted for against it.
#[test]
fn every_kind_reason_version_and_format_is_reachable_from_a_fixture() {
    let reports = scan(&documents());
    assert!(!reports.is_empty(), "the fixture directory was not walked");
    let rows = rows(&reports);

    // Kinds: named rows only. A kind that only ever appears on a refusal
    // is a kind this crate has never actually been willing to name.
    let named: BTreeSet<String> = rows
        .iter()
        .filter(|row| row["valid"] == true)
        .filter_map(|row| row["kind"].as_str().map(str::to_string))
        .collect();
    let kinds: Vec<String> = variants_of(POLICY, "Kind")
        .iter()
        .map(|variant| variant.to_lowercase())
        .collect();
    assert_eq!(kinds.len(), 5, "a kind arrived or left: {kinds:?}");
    for kind in &kinds {
        assert!(
            named.contains(kind),
            "no fixture document produces a {kind}: named {named:?}"
        );
    }
    assert!(
        named.iter().all(|kind| kinds.contains(kind)),
        "a kind was reported that the enum does not declare: {named:?} against {kinds:?}"
    );

    // Reasons: every refusal this crate can give, from a document.
    let refused: BTreeSet<String> = rows
        .iter()
        .filter_map(|row| row["refused"].as_str().map(str::to_string))
        .collect();
    let reasons: Vec<String> = variants_of(POLICY, "Reason")
        .iter()
        .map(|variant| snake_case(variant))
        .collect();
    assert_eq!(reasons.len(), 5, "a reason arrived or left: {reasons:?}");
    for reason in &reasons {
        assert!(
            refused.contains(reason),
            "no fixture document produces {reason}: refused {refused:?}"
        );
    }
    assert!(
        refused.iter().all(|reason| reasons.contains(reason)),
        "a reason was reported that the enum does not declare: {refused:?}"
    );

    // Variants: `rfc4122` from a named UUID, the other three from the
    // refusals that report which layout they found.
    let variants: BTreeSet<String> = rows
        .iter()
        .filter_map(|row| row["variant"].as_str().map(str::to_string))
        .collect();
    let declared: Vec<String> = variants_of(POLICY, "Variant")
        .iter()
        .map(|variant| snake_case(variant))
        .collect();
    assert_eq!(declared.len(), 4, "a variant arrived or left: {declared:?}");
    for variant in &declared {
        assert!(
            variants.contains(variant),
            "no fixture document reports the {variant} variant: {variants:?}"
        );
    }

    // Versions: every one the crate claims to recognise, **named** — a
    // version that only ever appears on a refusal is a version this
    // crate does not actually read.
    let versions: BTreeSet<i64> = rows
        .iter()
        .filter(|row| row["valid"] == true && row["kind"] == "uuid")
        .filter_map(|row| row["version"].as_i64())
        .collect();
    for version in VERSIONS {
        assert!(
            versions.contains(&version),
            "no fixture document holds a named UUID v{version}: {versions:?}"
        );
    }
    assert!(
        versions.iter().all(|version| VERSIONS.contains(version)),
        "a version outside RFC 9562's range was named: {versions:?}"
    );

    // Formats: every reader the schema offers, with at least one row out
    // of it. A format that opens a document and reports nothing is a
    // reader nobody is checking.
    let productive: BTreeSet<String> = reports
        .iter()
        .filter(|report| !report["ids"].as_array().expect("rows").is_empty())
        .filter_map(|report| report["format"].as_str().map(str::to_string))
        .collect();
    let offered = offered_formats();
    assert_eq!(offered.len(), 7, "a format arrived or left: {offered:?}");
    for format in &offered {
        assert!(
            productive.contains(format),
            "no fixture document is read as {format} and produces a row: {productive:?}"
        );
    }

    // The marker CI greps for. `cargo test <filter>` exits 0 on no
    // match, so without it a renamed test would leave a green job that
    // asserted nothing.
    println!(
        "coverage-matrix: complete — {} kinds, {} reasons, {} variants, {} uuid versions, \
         {} formats, from {} fixture documents",
        kinds.len(),
        reasons.len(),
        declared.len(),
        VERSIONS.len(),
        offered.len(),
        reports.len()
    );
}

/// **The false-positive rate, measured rather than asserted.**
///
/// An ObjectId's only evidence is its embedded timestamp, so a 24-hex
/// run whose leading four bytes land in the plausible window is named
/// one. The window covers roughly a quarter of the 32-bit second space,
/// which means roughly one random 24-hex run in four is named rather
/// than refused — and a repository full of abbreviated git object names
/// or truncated digests is full of random 24-hex runs.
///
/// `fixtures/documents/hashes.txt` is 600 of them, taken from real SHA-1
/// and MD5 digests. This test prints the measured rate and holds it to
/// the arithmetic: the share of the second space the window covers,
/// computed here from the same clock the binary reads. It does **not**
/// pin a number — the window's upper edge is `now + 365 days`, so the
/// rate rises by about 0.73 points a year on its own.
///
/// The behaviour is deliberate and documented in SPEC.md. This is the
/// measurement that would make tightening it a decision taken with a
/// number in hand.
#[test]
fn the_objectid_naming_rate_on_ordinary_hashes_follows_the_window() {
    let fixture = documents().join("hashes.txt");
    let reports = scan(&fixture);
    let rows = rows(&reports);

    let hex24: Vec<&serde_json::Value> = rows
        .iter()
        .filter(|row| row["value"].as_str().is_some_and(|value| value.len() == 24))
        .collect();
    assert!(
        hex24.len() >= 500,
        "the fixture stopped holding enough 24-hex runs to measure: {}",
        hex24.len()
    );

    let named = hex24.iter().filter(|row| row["valid"] == true).count();
    for row in &hex24 {
        // Whatever it decided, the decode is on the row — which is what
        // lets a reader disagree with a 2003 ObjectId in a document
        // written last week.
        assert!(
            row["timestamp"].is_string(),
            "a 24-hex verdict came without the decode behind it: {row}"
        );
    }

    let measured = f64::from(u32::try_from(named).expect("a count"))
        / f64::from(u32::try_from(hex24.len()).expect("a count"));
    let expected = window_share();
    println!(
        "coverage-matrix: 24-hex naming rate {:.1}% ({named} of {}) against a window covering \
         {:.1}% of the 32-bit second space — about one in {:.1}",
        measured * 100.0,
        hex24.len(),
        expected * 100.0,
        1.0 / measured.max(f64::EPSILON)
    );

    assert!(
        (measured - expected).abs() <= 0.10,
        "the measured rate {measured:.4} is nowhere near the window's {expected:.4} — \
         the rule changed, or the fixture is no longer random hex"
    );
}

/// The share of the 32-bit second space the plausibility window covers:
/// `[1990-01-01, now + 365 days]` out of `0 .. 2^32` seconds. The
/// constants are restated rather than imported — this test drives the
/// binary, and a constant shared with the code under test would move
/// with it.
fn window_share() -> f64 {
    const EARLIEST_PLAUSIBLE_S: u64 = 631_152_000;
    const FUTURE_ALLOWANCE_S: u64 = 365 * 86_400;
    const SECOND_SPACE: f64 = 4_294_967_296.0;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock")
        .as_secs();
    let upper = (now + FUTURE_ALLOWANCE_S).min(4_294_967_295);
    let width = u32::try_from(upper.saturating_sub(EARLIEST_PLAUSIBLE_S)).expect("a 32-bit width");
    f64::from(width) / SECOND_SPACE
}
