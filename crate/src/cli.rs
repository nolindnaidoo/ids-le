//! The terminal surface.
//!
//! stdout is always protocol — one JSON report per line, one line per
//! file. stderr is always for the human, and is a projection of the same
//! reports rather than parallel prose.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use crate::extract::{KIND_NAMES, Kind, resolve_format};
use crate::scan::{self, FileReport, ScanOptions};
use crate::walk::{self, WalkOptions};

const USAGE: &str = "usage: ids-le [options] <file|dir>...
       ids-le [options] --stdin [--format <format>]
       ids-le mcp
       ids-le --version | --help

Finds every identifier in a tree — UUID, ULID, NanoID, MongoDB ObjectId,
Snowflake — reports where it is and what it is, and decodes the time
inside the ones that carry one. JSON, YAML, CSV, TOML, INI and dotenv
also give each finding the document's own key path; anything else is read
as text and reported without one.

It refuses rather than guesses. A run that fits two schemes, a UUID whose
variant bits are wrong, the nil UUID, a v4 that is plainly not random, a
timestamp before 1990 — each is a row in the report with a named reason,
never a dropped row and never a silent answer.

Options:
  --kind <kind>        report only one of uuid, ulid, nanoid, objectid,
                       snowflake
  --format <format>    force a format instead of inferring it from the
                       file name; an unknown name falls back to a text
                       read rather than failing
  --strict             exit 2 if anything was refused or any file could
                       not be read, rather than reporting it and carrying
                       on
  --stdin              read one document from stdin
  --hidden             walk hidden files and directories too
  --no-ignore          walk files that .gitignore excludes

A binary file — a NUL byte in its first 8 KiB, ripgrep's own test — is
never a text candidate: it produces no report line, is counted on stderr,
and never fails the run. A file that looked like text and could not be
read is named on stderr, carried in the report, and does not fail the run
by itself.

Exit codes follow grep: 0 identifiers found · 1 none found · 2 malformed
question. A refusal is an answer, not an error, and does not move the
exit code unless --strict.";

/// Every flag the parser accepts. Held equal to the flags named in USAGE
/// by a test, and consulted at runtime so the list is what the parser
/// actually honours.
const FLAGS: [&str; 6] = [
    "--kind",
    "--format",
    "--strict",
    "--stdin",
    "--hidden",
    "--no-ignore",
];

#[derive(Debug)]
struct Cli {
    /// Fail the run on a refusal or an unreadable file.
    strict: bool,
    inputs: Vec<PathBuf>,
    stdin: bool,
    scan: ScanOptions,
    walk: WalkOptions,
}

pub(crate) fn run() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if let Some(first) = args.first() {
        match first.as_str() {
            "mcp" => return crate::mcp::serve(),
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--version" | "-V" => {
                println!("ids-le {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            _ => {}
        }
    }

    match execute(&args) {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("ids-le: {message}");
            ExitCode::from(2)
        }
    }
}

fn execute(args: &[String]) -> Result<u8, String> {
    let options = parse(args)?;
    let (reports, binary) = if options.stdin {
        (vec![scan_stdin(&options)?], 0)
    } else {
        let scanned = walk::collect(&options.inputs, &options.walk)?
            .iter()
            .map(|target| scan::scan_file(target, options.scan))
            .collect();
        scan::partition(scanned)
    };

    write_reports(&reports)?;
    summarise(&reports, binary);
    Ok(scan::exit_code(&reports, options.strict))
}

fn write_reports(reports: &[FileReport]) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    for report in reports {
        let line = serde_json::to_string(report).expect("a report serializes");
        writeln!(stdout, "{line}")
            .map_err(|error| format!("could not write the report: {error}"))?;
    }
    Ok(())
}

fn scan_stdin(options: &Cli) -> Result<FileReport, String> {
    let mut content = String::new();
    std::io::stdin()
        .read_to_string(&mut content)
        .map_err(|error| format!("could not read stdin: {error}"))?;
    // No filename to infer from, so an unnamed format is read as text —
    // which finds the same identifiers and only loses the key paths.
    let format = options
        .scan
        .format
        .unwrap_or(crate::extract::FALLBACK_FORMAT);
    Ok(scan::scan_content(
        scan::without_bom(&content),
        "<stdin>".to_string(),
        format,
        options.scan,
    ))
}

fn parse(args: &[String]) -> Result<Cli, String> {
    let mut options = Cli {
        inputs: Vec::new(),
        stdin: false,
        strict: false,
        scan: ScanOptions::default(),
        walk: WalkOptions::default(),
    };

    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        // Strict parsing, never a silent default: a typo'd `--strick`
        // that quietly did nothing would produce a report the caller
        // believed had been held to a standard.
        if arg.starts_with('-') && !FLAGS.contains(&arg.as_str()) {
            return Err(format!("{arg} is not an option. Try --help."));
        }

        match arg.as_str() {
            "--stdin" => options.stdin = true,
            "--strict" => options.strict = true,
            "--hidden" => options.walk.hidden = true,
            "--no-ignore" => options.walk.respect_ignore = false,
            // An unknown format falls back rather than failing, which is
            // the one place this tool is lenient and the reason it can
            // be pointed at a repository nobody has described to it. The
            // flag still takes a value, so a missing one is a refusal.
            "--format" => {
                let value = rest
                    .next()
                    .ok_or_else(|| "--format needs a format".to_string())?;
                options.scan.format = Some(resolve_format(Some(value), None));
            }
            // A kind, unlike a format, is **not** lenient. An unknown
            // format costs key paths; an unknown kind would silently
            // report nothing at all, and a caller reading an empty
            // report would conclude the tree is clean.
            "--kind" => {
                let value = rest
                    .next()
                    .ok_or_else(|| format!("--kind needs one of {}", KIND_NAMES.join(", ")))?;
                let kind = Kind::from_name(value).ok_or_else(|| {
                    format!(
                        "{value} is not a kind. Try one of {}.",
                        KIND_NAMES.join(", ")
                    )
                })?;
                options.scan = options.scan.with_kind(Some(kind));
            }
            path => options.inputs.push(PathBuf::from(path)),
        }
    }

    if options.stdin && !options.inputs.is_empty() {
        return Err("reading from stdin takes no file arguments".to_string());
    }
    if !options.stdin && options.inputs.is_empty() {
        return Err("name a file or a directory to read. Try --help.".to_string());
    }
    Ok(options)
}

/// The human half. Every line restates something already on stdout —
/// except the binary count, which is the one thing stdout cannot carry
/// because those files produce no report line at all.
fn summarise(reports: &[FileReport], binary: usize) {
    let mut stderr = std::io::stderr().lock();
    let mut ids = 0;
    let mut refused = 0;

    for report in reports {
        for diagnostic in &report.diagnostics {
            let _ = writeln!(stderr, "{}: {}", report.file, diagnostic.message);
        }
        for found in &report.ids {
            let _ = writeln!(stderr, "{}", scan::describe(report, found));
        }
        ids += report.summary.ids;
        refused += report.summary.refused;
    }

    let _ = writeln!(
        stderr,
        "{} in {}",
        plural(ids, "identifier", "identifiers"),
        plural(reports.len(), "file", "files")
    );
    if refused > 0 {
        // Said plainly and always: a reader treating this report as a
        // complete index of the identifiers in a tree needs to know how
        // much of it this tool declined to name.
        let _ = writeln!(stderr, "{} refused", plural(refused, "run", "runs"));
    }
    if binary > 0 {
        let _ = writeln!(
            stderr,
            "{} skipped",
            plural(binary, "binary file", "binary files")
        );
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::SUPPORTED_FORMATS;

    #[test]
    fn every_documented_flag_is_parsed_and_the_reverse() {
        let mut documented: Vec<&str> = USAGE
            .split_whitespace()
            .filter(|word| word.starts_with("--"))
            .map(|word| word.trim_end_matches([',', '.', ':', ';']))
            .filter(|word| !matches!(*word, "--version" | "--help"))
            .collect();
        documented.sort_unstable();
        documented.dedup();

        let mut implemented = FLAGS.to_vec();
        implemented.sort_unstable();
        assert_eq!(documented, implemented);
    }

    #[test]
    fn the_parser_accepts_every_flag_it_lists() {
        for flag in FLAGS {
            let args: Vec<String> = match flag {
                "--format" => vec![flag.into(), "json".into(), "x".into()],
                "--kind" => vec![flag.into(), "uuid".into(), "x".into()],
                "--stdin" => vec![flag.into()],
                _ => vec![flag.into(), "x".into()],
            };
            assert!(parse(&args).is_ok(), "{flag}");
        }
    }

    #[test]
    fn an_unknown_flag_is_refused_rather_than_ignored() {
        let error = parse(&["--strick".into(), "x".into()]).expect_err("a refusal");
        assert!(error.contains("--strick"), "{error}");
    }

    /// The one place this crate is deliberately lenient: a format
    /// nobody recognises is a text read, not a refusal.
    #[test]
    fn an_unknown_format_falls_back_rather_than_being_refused() {
        let options =
            parse(&["--format".into(), "handwriting".into(), "x".into()]).expect("accepted");
        assert_eq!(options.scan.format, Some(crate::extract::FALLBACK_FORMAT));
    }

    #[test]
    fn every_offered_format_is_accepted_by_name() {
        for format in SUPPORTED_FORMATS {
            let options = parse(&["--format".into(), format.into(), "x".into()]).expect(format);
            assert_eq!(options.scan.format, Some(format));
        }
    }

    /// The asymmetry, said as a test: leniency about a format costs key
    /// paths, leniency about a kind would report an empty tree.
    #[test]
    fn an_unknown_kind_is_refused_where_an_unknown_format_is_not() {
        let error = parse(&["--kind".into(), "guid".into(), "x".into()]).expect_err("a refusal");
        assert!(error.contains("guid"), "{error}");
        assert!(
            error.contains("uuid"),
            "the alternatives are named: {error}"
        );
    }

    #[test]
    fn every_offered_kind_is_accepted_by_name() {
        for kind in KIND_NAMES {
            let options = parse(&["--kind".into(), kind.into(), "x".into()]).expect(kind);
            assert_eq!(options.scan.extract.kind, Kind::from_name(kind));
        }
    }

    #[test]
    fn a_flag_with_no_value_is_refused() {
        assert!(parse(&["--format".into()]).is_err());
        assert!(parse(&["--kind".into()]).is_err());
    }

    /// This tool names what is there and refuses what it cannot name.
    /// There is no flag that asks it to decide, rewrite or generate.
    #[test]
    fn no_flag_asks_for_a_judgment_or_a_change() {
        for attempt in [
            "--fix",
            "--generate",
            "--redact",
            "--guess",
            "--best-effort",
        ] {
            assert!(
                parse(&[attempt.into(), "x".into()]).is_err(),
                "{attempt} was accepted"
            );
        }
        for word in ["generate", "rewrite", "redact", "best-effort"] {
            assert!(!USAGE.contains(word), "the usage text offers {word}");
        }
        assert!(
            USAGE.contains("refuses rather than guesses"),
            "the one thing this tool promises is undocumented"
        );
    }

    #[test]
    fn naming_nothing_is_refused() {
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn stdin_and_file_arguments_together_are_refused() {
        assert!(parse(&["--stdin".into(), "x".into()]).is_err());
    }

    #[test]
    fn the_usage_text_states_greps_convention_and_the_refusal_rule() {
        assert!(USAGE.contains("grep"));
        for code in ["0", "1", "2"] {
            assert!(USAGE.contains(code), "exit code {code} is undocumented");
        }
        assert!(
            USAGE.contains("exit code unless --strict"),
            "the rule a caller scripts against is undocumented"
        );
    }

    /// Every kind the tool can report is named in the help, or a caller
    /// has no way to learn what `--kind` takes.
    #[test]
    fn the_usage_text_names_every_kind() {
        for kind in KIND_NAMES {
            assert!(USAGE.contains(kind), "{kind} is undocumented");
        }
    }
}
