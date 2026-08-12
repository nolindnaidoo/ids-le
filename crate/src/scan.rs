//! One file end to end — the only path either surface calls.
//!
//! `cli.rs` and `mcp/` both come through here, so a rule can only be
//! written once. `tests/contracts.rs` asserts the two agree.
//!
//! This is also where the clock lives. `extract/` is handed a `Clock`
//! rather than reading one, so the plausibility window is a value a test
//! can pin; the wall clock is read exactly here, on the impure side of
//! the line.

use std::path::{Path as StdPath, PathBuf};

use serde::Serialize;

use crate::extract::{self, Clock, Found, Kind, Options, resolve_format};

/// The version of the report shape on stdout.
///
/// On every line, not inferred from the fields present. A consumer
/// branching on the shape needs to branch on something that is there
/// even when a document produced nothing.
const SCHEMA: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Diagnostic {
    pub(crate) severity: String,
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct Summary {
    /// Rows this crate was willing to name.
    pub(crate) ids: usize,
    /// Rows it refused. Reported rather than inferred: a reader deciding
    /// whether this report is a complete index of the identifiers in a
    /// tree needs to know how much of it this tool declined to name, and
    /// a silent zero and a silent forty look identical.
    pub(crate) refused: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FileReport {
    pub(crate) schema: u8,
    pub(crate) file: String,
    pub(crate) format: String,
    pub(crate) ids: Vec<Found>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) summary: Summary,
}

impl FileReport {
    /// Whether this file was not read at all — not text, or not
    /// openable.
    ///
    /// Reported rather than swallowed, because a report that quietly
    /// skipped a file would be claiming coverage it does not have. It
    /// does **not** fail the run on its own: every repository has a PNG
    /// and a zip in it, and exiting 2 on those makes the tool unusable
    /// in CI, which is the one place it is most worth running.
    pub(crate) fn was_skipped(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "skipped")
    }

    /// Whether the scan of this file gave up part way. Unlike a skip
    /// this **does** fail the run: reporting no findings for a file that
    /// was never finished would overstate coverage, which is the one
    /// thing an audit tool must never do.
    pub(crate) fn is_incomplete(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == "error")
    }
}

/// Now, in Unix milliseconds.
///
/// A clock that cannot be read is treated as the epoch, which closes the
/// plausibility window to nothing and turns every decoded timestamp into
/// a refusal. That is the safe direction: a broken clock should make
/// this tool say less, never more.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScanOptions {
    pub(crate) extract: Options,
    /// A format the caller forced, instead of one inferred per file.
    pub(crate) format: Option<&'static str>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            extract: Options {
                clock: Clock::at(now_ms()),
                kind: None,
            },
            format: None,
        }
    }
}

impl ScanOptions {
    pub(crate) fn with_kind(self, kind: Option<Kind>) -> Self {
        Self {
            extract: Options {
                kind,
                ..self.extract
            },
            ..self
        }
    }
}

/// What reading one file produced.
///
/// **A binary file is not a report.** It was never a text candidate —
/// every repository holds a PNG — and reporting it as a file that could
/// not be read would make `--strict` exit 2 on any tree containing an
/// image. It is counted instead, so the reader still knows coverage was
/// narrower than the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Scanned {
    Read(Box<FileReport>),
    Binary,
}

impl Scanned {
    pub(crate) fn into_report(self) -> Option<FileReport> {
        match self {
            Self::Read(report) => Some(*report),
            Self::Binary => None,
        }
    }
}

/// Split what a walk produced into the reports and the count of files
/// that were never text. Both surfaces come through here so neither can
/// grow its own idea of what a binary file is.
pub(crate) fn partition(scanned: Vec<Scanned>) -> (Vec<FileReport>, usize) {
    let binary = scanned
        .iter()
        .filter(|one| **one == Scanned::Binary)
        .count();
    let reports = scanned
        .into_iter()
        .filter_map(Scanned::into_report)
        .collect();
    (reports, binary)
}

/// ripgrep's heuristic, and deliberately the same one: a NUL byte in the
/// first 8 KiB means binary. "What this tool opens" and "what ripgrep
/// opens" being the same answer is already the rule `walk.rs` follows.
const BINARY_SNIFF_BYTES: usize = 8192;

fn is_binary(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .take(BINARY_SNIFF_BYTES)
        .any(|byte| *byte == b'\0')
}

pub(crate) fn scan_file(path: &PathBuf, options: ScanOptions) -> Scanned {
    let file = path.to_string_lossy().into_owned();
    let format = options.format.unwrap_or_else(|| format_of(path));

    match std::fs::read(path) {
        Ok(bytes) if is_binary(&bytes) => Scanned::Binary,
        Ok(bytes) => Scanned::Read(Box::new(match String::from_utf8(bytes) {
            Ok(content) => scan_content(without_bom(&content), file, format, options),
            // Named rather than dropped. A file that looked like text and
            // was not is a file the reader would otherwise believe was
            // covered.
            Err(_) => skipped(file, format, "not UTF-8 text"),
        })),
        Err(error) => Scanned::Read(Box::new(skipped(file, format, &error.to_string()))),
    }
}

fn format_of(path: &StdPath) -> &'static str {
    resolve_format(None, path.file_name().and_then(|name| name.to_str()))
}

pub(crate) fn scan_content(
    content: &str,
    file: String,
    format: &str,
    options: ScanOptions,
) -> FileReport {
    let ids = extract::extract(content, format, options.extract);
    let (named, refused) = extract::counts(&ids);
    FileReport {
        schema: SCHEMA,
        file,
        format: format.to_string(),
        ids,
        diagnostics: Vec::new(),
        summary: Summary {
            ids: named,
            refused,
        },
    }
}

/// grep's convention: 0 found, 1 none found, 2 could not answer.
///
/// **A refusal does not move the exit code.** Refusing is this tool
/// working, not failing — a tree full of ambiguous 24-hex runs is an
/// answer about that tree. `--strict` is for the pipeline that wants
/// every row named or nothing shipped, and it turns both a refusal and
/// an unreadable text file into a failure.
pub(crate) fn exit_code(reports: &[FileReport], strict: bool) -> u8 {
    // A scan that gave up part way always fails: it would otherwise
    // report "nothing found" for a file it never finished reading.
    if reports.iter().any(FileReport::is_incomplete) {
        return 2;
    }
    if strict
        && reports
            .iter()
            .any(|report| report.was_skipped() || report.summary.refused > 0)
    {
        return 2;
    }
    u8::from(!reports.iter().any(|report| report.summary.ids > 0))
}

/// One row, for the human half of the output.
pub(crate) fn describe(report: &FileReport, found: &Found) -> String {
    let where_it_is = format!(
        "{}:{}:{}",
        report.file, found.position.line, found.position.column
    );
    let Some(reason) = found.refused else {
        let kind = found
            .kind
            .map_or_else(|| "id".to_string(), |kind| kind_name(kind).to_string());
        let version = found
            .version
            .map_or_else(String::new, |version| format!(" v{version}"));
        let when = found
            .timestamp
            .as_deref()
            .map_or_else(String::new, |stamp| format!("  {stamp}"));
        return format!("{where_it_is}  {kind}{version}  {}{when}", found.value);
    };
    format!(
        "{where_it_is}  refused ({})  {}  — {}",
        reason_name(reason),
        found.value,
        found.detail.as_deref().unwrap_or("no reason given")
    )
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Uuid => "uuid",
        Kind::Ulid => "ulid",
        Kind::NanoId => "nanoid",
        Kind::ObjectId => "objectid",
        Kind::Snowflake => "snowflake",
    }
}

fn reason_name(reason: crate::extract::Reason) -> &'static str {
    use crate::extract::Reason;
    match reason {
        Reason::AmbiguousKind => "ambiguous_kind",
        Reason::Malformed => "malformed",
        Reason::NilOrMax => "nil_or_max",
        Reason::VersionClaimMismatch => "version_claim_mismatch",
        Reason::TimestampImplausible => "timestamp_implausible",
    }
}

/// The report for a file that was not read: named, warned about, and
/// not a failure by itself.
fn skipped(file: String, format: &'static str, reason: &str) -> FileReport {
    FileReport {
        schema: SCHEMA,
        file,
        format: format.to_string(),
        ids: Vec::new(),
        diagnostics: vec![Diagnostic {
            severity: "warning".to_string(),
            code: "skipped".to_string(),
            message: reason.to_string(),
        }],
        summary: Summary { ids: 0, refused: 0 },
    }
}

/// Drop a leading byte-order mark.
///
/// No editor shows it, and without this three invisible bytes shift
/// every column on the first line — which in a structured format can
/// lose the document entirely. Notepad, Excel and a PowerShell redirect
/// all add one.
pub(crate) fn without_bom(content: &str) -> &str {
    content.strip_prefix('\u{feff}').unwrap_or(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::Reason;
    use crate::testing::TempTree;

    const UUID: &str = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
    const NIL: &str = "00000000-0000-0000-0000-000000000000";

    /// A pinned clock, so nothing here depends on the day it runs.
    fn plain() -> ScanOptions {
        ScanOptions {
            extract: Options {
                clock: Clock::at(1_786_492_800_000),
                kind: None,
            },
            format: None,
        }
    }

    fn read(path: &PathBuf, options: ScanOptions) -> FileReport {
        scan_file(path, options)
            .into_report()
            .expect("the file was a text candidate")
    }

    #[test]
    fn a_document_with_identifiers_exits_zero() {
        let report = scan_content(
            &format!("{{\"id\":\"{UUID}\"}}"),
            "a.json".into(),
            "json",
            plain(),
        );
        assert_eq!(report.summary.ids, 1);
        assert_eq!(report.schema, 1);
        assert_eq!(exit_code(&[report], false), 0);
    }

    #[test]
    fn a_document_with_none_exits_one() {
        let report = scan_content(r#"{"a":"text"}"#, "a.json".into(), "json", plain());
        assert_eq!(report.summary.ids, 0);
        assert_eq!(exit_code(&[report], false), 1);
    }

    #[test]
    fn nothing_to_scan_exits_one() {
        assert_eq!(exit_code(&[], false), 1);
    }

    /// Refusing is this tool working. A document of nothing but
    /// refusals is an answer about that document, not a failed run —
    /// until a pipeline asks for `--strict`.
    #[test]
    fn a_refusal_does_not_move_the_exit_code_unless_strict() {
        let report = scan_content(
            &format!("{{\"id\":\"{NIL}\"}}"),
            "a.json".into(),
            "json",
            plain(),
        );
        assert_eq!(report.summary, Summary { ids: 0, refused: 1 });
        assert_eq!(report.ids[0].refused, Some(Reason::NilOrMax));
        assert_eq!(exit_code(std::slice::from_ref(&report), false), 1);
        assert_eq!(exit_code(&[report], true), 2);
    }

    /// A found identifier beside a refusal still exits 0 without
    /// `--strict`: something was found.
    #[test]
    fn a_refusal_beside_a_finding_still_exits_zero() {
        let report = scan_content(
            &format!("a: {UUID}\nb: {NIL}\n"),
            "a.yaml".into(),
            "yaml",
            plain(),
        );
        assert_eq!(report.summary, Summary { ids: 1, refused: 1 });
        assert_eq!(exit_code(std::slice::from_ref(&report), false), 0);
        assert_eq!(exit_code(&[report], true), 2, "--strict is opt-in");
    }

    #[test]
    fn an_unreadable_file_is_reported_and_does_not_end_the_run() {
        let tree = TempTree::new("scan-unreadable");
        let report = read(&tree.path().join("gone.json"), plain());
        assert!(report.was_skipped());
        assert_eq!(report.diagnostics[0].severity, "warning");
        assert_eq!(exit_code(std::slice::from_ref(&report), false), 1);
        assert_eq!(exit_code(&[report], true), 2, "--strict is opt-in");
    }

    #[test]
    fn a_binary_file_is_not_a_report() {
        let tree = TempTree::new("scan-binary");
        let file = tree.write_bytes("logo.png", &[0x89, 0x50, 0x4e, 0x47, 0x00, 0x1a]);
        assert_eq!(scan_file(&file, plain()), Scanned::Binary);
    }

    /// The distinction the split exists for: a file that *is* text and
    /// could not be read keeps its named diagnostic and keeps failing
    /// `--strict`. A PNG beside it does neither.
    #[test]
    fn a_text_file_that_cannot_be_read_fails_strict_and_a_binary_one_does_not() {
        let tree = TempTree::new("scan-strict");
        let binary = tree.write_bytes("logo.png", &[0x89, 0x50, 0x00, 0xff]);
        // Invalid UTF-8 with no NUL byte: it looked like text and was not.
        let broken = tree.write_bytes("notes.txt", &[0x68, 0x69, 0xff, 0xfe]);
        let good = tree.write("ids.env", &format!("SERVICE_ID={UUID}\n"));

        let (reports, binaries) = partition(vec![
            scan_file(&binary, plain()),
            scan_file(&broken, plain()),
            scan_file(&good, plain()),
        ]);
        assert_eq!(binaries, 1);
        assert_eq!(reports.len(), 2, "the PNG produced no report line");
        assert_eq!(reports[0].diagnostics[0].message, "not UTF-8 text");
        assert_eq!(exit_code(&reports, false), 0, "the .env file has an id");
        assert_eq!(exit_code(&reports, true), 2, "the unreadable text file");

        let binary_only = partition(vec![scan_file(&binary, plain())]).0;
        assert_eq!(
            exit_code(&binary_only, true),
            1,
            "a binary file never fails --strict"
        );
    }

    /// ripgrep's own test, and the reason it is that one: a NUL byte
    /// after the first 8 KiB belongs to a file this already read as
    /// text.
    #[test]
    fn binary_is_a_nul_byte_in_the_first_8_kib() {
        let tree = TempTree::new("scan-sniff");
        let mut late = vec![b'a'; BINARY_SNIFF_BYTES + 16];
        late[BINARY_SNIFF_BYTES + 8] = 0;
        let file = tree.write_bytes("late.txt", &late);
        assert_ne!(scan_file(&file, plain()), Scanned::Binary);
    }

    #[test]
    fn the_format_comes_from_the_file_name() {
        let tree = TempTree::new("scan-format");
        let file = tree.write("config.toml", &format!("id = \"{UUID}\"\n"));
        let report = read(&file, plain());
        assert_eq!(report.format, "toml");
        assert_eq!(report.ids[0].key.as_deref(), Some("id"));
    }

    /// The property that makes this tool usable on a tree nobody has
    /// described: an unrecognised file is still scanned, and only its
    /// key paths are missing.
    #[test]
    fn an_unrecognised_file_is_still_scanned() {
        let tree = TempTree::new("scan-fallback");
        let file = tree.write("NOTES.md", &format!("Released under {UUID}.\n"));
        let report = read(&file, plain());
        assert_eq!(report.format, "text");
        assert_eq!(report.summary.ids, 1);
        assert_eq!(report.ids[0].key, None);
    }

    #[test]
    fn a_forced_format_overrides_the_file_name() {
        let tree = TempTree::new("scan-forced");
        let file = tree.write("data.json", &format!("id = \"{UUID}\"\n"));
        let report = read(
            &file,
            ScanOptions {
                format: Some("toml"),
                ..plain()
            },
        );
        assert_eq!(report.format, "toml");
        assert_eq!(report.ids[0].key.as_deref(), Some("id"));
    }

    #[test]
    fn a_kind_filter_narrows_the_report() {
        let text = format!("a: {UUID}\nb: 01KZSM9K00ABCDEFGH12345678\n");
        let all = scan_content(&text, "a.yaml".into(), "yaml", plain());
        assert_eq!(all.summary.ids, 2);
        let only = scan_content(
            &text,
            "a.yaml".into(),
            "yaml",
            plain().with_kind(Some(Kind::Uuid)),
        );
        assert_eq!(only.summary.ids, 1);
    }

    #[test]
    fn the_human_line_carries_the_kind_and_the_decode() {
        let report = scan_content(
            "id: 019ff344-cc00-7abc-8def-0123456789ab\n",
            "a.yaml".into(),
            "yaml",
            plain(),
        );
        assert_eq!(
            describe(&report, &report.ids[0]),
            "a.yaml:1:5  uuid v7  019ff344-cc00-7abc-8def-0123456789ab  2026-08-12T00:00:00.000Z"
        );
    }

    #[test]
    fn the_human_line_names_the_refusal_and_its_reason() {
        let report = scan_content(&format!("id: {NIL}\n"), "a.yaml".into(), "yaml", plain());
        let line = describe(&report, &report.ids[0]);
        assert!(line.contains("refused (nil_or_max)"), "{line}");
        assert!(line.contains("RFC 9562"), "{line}");
    }

    /// Every reason has a name on the human surface, or a reader sees a
    /// refusal with no word for it.
    #[test]
    fn every_reason_has_a_name_matching_its_serialized_form() {
        for reason in [
            Reason::AmbiguousKind,
            Reason::Malformed,
            Reason::NilOrMax,
            Reason::VersionClaimMismatch,
            Reason::TimestampImplausible,
        ] {
            assert_eq!(
                serde_json::to_value(reason).expect("serializes"),
                reason_name(reason)
            );
        }
    }

    #[test]
    fn every_kind_has_a_name_matching_its_serialized_form() {
        for kind in [
            Kind::Uuid,
            Kind::Ulid,
            Kind::NanoId,
            Kind::ObjectId,
            Kind::Snowflake,
        ] {
            assert_eq!(
                serde_json::to_value(kind).expect("serializes"),
                kind_name(kind)
            );
        }
    }
}

#[cfg(test)]
mod hazards {
    use super::*;

    /// Three invisible bytes that Notepad, Excel and a PowerShell
    /// redirect all add, and that would otherwise shift every column on
    /// the first line.
    #[test]
    fn a_byte_order_mark_is_not_part_of_the_document() {
        assert_eq!(without_bom("\u{feff}abc"), "abc");
        assert_eq!(without_bom("abc"), "abc");
        // Only a leading one: elsewhere it is a zero-width no-break
        // space and belongs to the text.
        assert_eq!(without_bom("a\u{feff}b"), "a\u{feff}b");
    }

    /// A clock that could not be read closes the window rather than
    /// opening it. Said as a test because the safe direction is not
    /// obvious from the code.
    #[test]
    fn an_unreadable_clock_makes_the_tool_say_less() {
        let closed = Clock::at(0);
        assert!(!closed.is_plausible(crate::extract::policy::EARLIEST_PLAUSIBLE_MS));
    }
}
