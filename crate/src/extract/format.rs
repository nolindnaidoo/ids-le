//! Which key-path reader a document gets.
//!
//! **An unresolved format is not an error.** The identifier scan itself
//! is the same for every document — an identifier is a run of
//! characters, not a typed value — so the format decides one thing only:
//! whether a finding can be given a key path. A Markdown file, a log, a
//! `.ts` file all fall through to the plain-text reader, which finds the
//! same identifiers and reports them without a key.
//!
//! That is a smaller job than the same file does in `numbers-le`, and
//! the difference is the point: there, the format decides what *counts*
//! as a number, because `"42"` in JSON is a string and in `.env` is a
//! number. An identifier has no such fork — `550e8400-…` is the same
//! identifier quoted or bare — so no format here can change which
//! findings exist, only how they are addressed.

/// Which key-path reader a document gets.
///
/// **A type, not the reader's name as a string.** `locate::key_spans`
/// dispatched on `&str` with a `_ => Vec::new()` catch-all, so a reader
/// added to this file and not wired up over there would have cost every
/// key path in that format and failed no build — and a key path is
/// evidence, so it would have cost verdicts too. Three tests caught that
/// drift between them; a variant that does not compile is better than
/// three tests that notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reader {
    Json,
    Yaml,
    Toml,
    Ini,
    Dotenv,
    Csv,
    /// The same reader with a tab between fields. Separate because the
    /// header row is the evidence that names a column an identifier, and
    /// splitting a tab row on commas made the whole row one column name.
    Tsv,
    /// No keys at all. The honest answer for a `.md` or a `.rs`, not a
    /// degraded mode — see `FALLBACK_FORMAT`.
    Text,
}

impl Reader {
    /// The name this reader goes by on the wire, in `--format` and in the
    /// tool schema. One place, so the report and the flag cannot
    /// disagree.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Ini => "ini",
            Self::Dotenv => "env",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Text => "text",
        }
    }
}

/// Every name a caller might send, mapped to the reader it means.
///
/// Both a VS Code `languageId` and a file extension appear here, because
/// an editor resolves by the first and this crate by the second.
/// `conf` and `cfg` are deliberately absent. They named the INI reader,
/// which reads `key: value` out of free-form prose — so "The failing
/// request id: 6a7bb780…" became a key that named the run an ObjectId,
/// and an English sentence decided a verdict. They fall to the text
/// scan, which finds the same run and refuses it for want of a name.
const ALIASES: [(&str, Reader); 13] = [
    ("json", Reader::Json),
    ("jsonc", Reader::Json),
    ("yaml", Reader::Yaml),
    ("yml", Reader::Yaml),
    ("csv", Reader::Csv),
    ("tsv", Reader::Tsv),
    ("toml", Reader::Toml),
    ("ini", Reader::Ini),
    ("properties", Reader::Ini),
    ("env", Reader::Dotenv),
    ("dotenv", Reader::Dotenv),
    ("text", Reader::Text),
    ("txt", Reader::Text),
];

/// The formats a caller can name, for the tool schema's enum. Held equal
/// to the alias table by a test, so a format can never be offered and
/// then not resolve.
pub(crate) const SUPPORTED_FORMATS: [&str; 8] =
    ["json", "yaml", "csv", "tsv", "toml", "ini", "env", "text"];

/// What the engine uses when it recognises nothing.
///
/// **`text`, not `unknown`.** Unlike the sibling crates, falling through
/// here is not a degraded mode — the scan finds exactly the same
/// identifiers it finds in a JSON file, and only the key path is missing.
/// Naming it `unknown` would tell a reader the document was not
/// understood, when what actually happened is that it had no keys to
/// report.
pub(crate) const FALLBACK_FORMAT: &str = Reader::Text.name();

/// Case folded last, so the whole thing is one allocation: no lowercase
/// mapping produces or consumes a leading `.`, which is the only
/// character stripped here.
fn normalise(value: &str) -> String {
    value.trim().trim_start_matches('.').to_lowercase()
}

/// The reader for an already-canonical format name, or the fallback.
/// Used on the hot path, where the caller has resolved once.
pub(crate) fn canonical(format: &str) -> Reader {
    ALIASES
        .iter()
        .find(|(alias, _)| *alias == format)
        .map_or(Reader::Text, |(_, reader)| *reader)
}

/// Resolve a reader key from an explicit format, else from a filename,
/// else the fallback.
pub(crate) fn resolve_format(format: Option<&str>, filename: Option<&str>) -> &'static str {
    resolve_reader(format, filename).name()
}

fn resolve_reader(format: Option<&str>, filename: Option<&str>) -> Reader {
    if let Some(name) = format {
        let direct = canonical(&normalise(name));
        if direct != Reader::Text {
            return direct;
        }
    }

    let Some(filename) = filename else {
        return Reader::Text;
    };

    // A dotfile like `.env` has no extension to split on; its whole name
    // is the type.
    let whole = canonical(&normalise(filename));
    if whole != Reader::Text {
        return whole;
    }

    // **A dotenv file is `.env` and everything after it.** Splitting on
    // the last dot asks `local` for a format and gets nothing, so
    // `.env.local` fell to the plain-text reader — which has no keys.
    // A key path is evidence for four kinds here, so this did not merely
    // lose a locator: `USER_ID=6a7bb780a1b2c3d4e5f60718` was a named
    // ObjectId in `.env` and a refusal in the `.env.local` beside it.
    if is_dotenv(&filename.trim().to_lowercase()) {
        return Reader::Dotenv;
    }

    filename
        .rsplit_once('.')
        .map_or(Reader::Text, |(_, extension)| {
            canonical(&normalise(extension))
        })
}

/// Whether a filename names a dotenv file.
///
/// `.env` and any suffix of it — `.env.local`, `.env.production`,
/// `.env.test.local` — plus the `<name>.env` spelling.
///
/// **The leading dot is the signal**, so this takes the name before
/// `normalise` strips it. Without it `env.ts` would read as dotenv,
/// which is the worse mistake: it hands a source file a key grammar it
/// does not have. `.envrc` is direnv's shell script, not a dotenv file.
fn is_dotenv(name: &str) -> bool {
    name == ".env"
        || name.starts_with(".env.")
        || name == "env"
        || name
            .strip_suffix(".env")
            .is_some_and(|stem| !stem.is_empty())
}

#[cfg(test)]
mod dotenv_tests {
    use super::{Reader, resolve_reader as resolve};

    /// The key path is evidence for four kinds, so falling to the
    /// plain-text reader turned a named ObjectId into a refusal.
    #[test]
    fn every_dotenv_spelling_resolves() {
        for name in [
            ".env",
            ".env.local",
            ".env.production",
            ".env.test.local",
            "app.env",
            "env",
        ] {
            assert_eq!(resolve(None, Some(name)), Reader::Dotenv, "{name}");
        }
    }

    #[test]
    fn a_name_that_merely_starts_with_env_is_not_dotenv() {
        for name in [".envrc", "environment.json", "env.ts", "sender.env.rs"] {
            assert_ne!(resolve(None, Some(name)), Reader::Dotenv, "{name}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_offered_format_resolves_to_itself() {
        for format in SUPPORTED_FORMATS {
            assert_eq!(resolve_format(Some(format), None), format, "{format}");
        }
    }

    #[test]
    fn the_common_aliases_are_honoured() {
        for (alias, expected) in [
            ("jsonc", "json"),
            ("yml", "yaml"),
            ("tsv", "tsv"),
            ("properties", "ini"),
            ("dotenv", "env"),
        ] {
            assert_eq!(resolve_format(Some(alias), None), expected, "{alias}");
        }
    }

    #[test]
    fn a_name_is_normalised_before_it_is_matched() {
        assert_eq!(resolve_format(Some("  JSON "), None), "json");
        assert_eq!(resolve_format(Some(".toml"), None), "toml");
    }

    #[test]
    fn a_filename_supplies_the_format_when_none_is_named() {
        assert_eq!(resolve_format(None, Some("config.toml")), "toml");
        assert_eq!(resolve_format(None, Some("data.CSV")), "csv");
    }

    #[test]
    fn a_dotfile_resolves_by_its_whole_name() {
        assert_eq!(resolve_format(None, Some(".env")), "env");
        assert_eq!(resolve_format(None, Some("env")), "env");
    }

    /// Not a refusal, not an empty result — the plain-text reader, which
    /// finds the same identifiers and reports them without a key.
    #[test]
    fn anything_unrecognised_falls_back() {
        // `conf` and `cfg` are here on purpose: they named the INI
        // reader, which found keys in prose and let a sentence decide a
        // verdict.
        for name in ["markdown", "dockerfile", "", "wat", "conf", "cfg"] {
            assert_eq!(resolve_format(Some(name), None), FALLBACK_FORMAT, "{name}");
        }
        assert_eq!(resolve_format(None, Some("README.md")), FALLBACK_FORMAT);
        assert_eq!(resolve_format(None, Some("main.rs")), FALLBACK_FORMAT);
        assert_eq!(resolve_format(None, None), FALLBACK_FORMAT);
    }

    /// An explicit format that resolves to nothing still lets the
    /// filename answer, rather than the bad name poisoning the lookup.
    #[test]
    fn an_unresolved_format_defers_to_the_filename() {
        assert_eq!(resolve_format(Some("nonsense"), Some("a.toml")), "toml");
    }

    #[test]
    fn the_offered_list_matches_the_alias_table() {
        for format in SUPPORTED_FORMATS {
            assert!(
                ALIASES.iter().any(|(_, reader)| reader.name() == format),
                "{format} is offered but no alias produces it"
            );
        }
        for (_, reader) in ALIASES {
            assert!(
                SUPPORTED_FORMATS.contains(&reader.name()),
                "{} is produced but not offered",
                reader.name()
            );
        }
    }

    /// **Every reader is offered, and every offered name resolves back to
    /// it.** The match is exhaustive on purpose: a new variant does not
    /// compile until it is named here, does not pass until
    /// `SUPPORTED_FORMATS` offers it, and does not compile at all until
    /// `locate::key_spans` says what it does. That chain is what replaced
    /// a `_ => Vec::new()` nobody would have noticed going quiet.
    #[test]
    fn every_reader_is_offered_and_resolves_back_to_itself() {
        for reader in [
            Reader::Json,
            Reader::Yaml,
            Reader::Toml,
            Reader::Ini,
            Reader::Dotenv,
            Reader::Csv,
            Reader::Tsv,
            Reader::Text,
        ] {
            let name = match reader {
                Reader::Json => "json",
                Reader::Yaml => "yaml",
                Reader::Toml => "toml",
                Reader::Ini => "ini",
                Reader::Dotenv => "env",
                Reader::Csv => "csv",
                Reader::Tsv => "tsv",
                Reader::Text => "text",
            };
            assert_eq!(reader.name(), name);
            assert!(
                SUPPORTED_FORMATS.contains(&name),
                "{name} is a reader and is not offered"
            );
            assert_eq!(canonical(name), reader, "{name} does not resolve to itself");
        }
    }
}
