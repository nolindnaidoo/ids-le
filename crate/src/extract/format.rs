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

/// Every name a caller might send, mapped to the reader key it means.
///
/// Both a VS Code `languageId` and a file extension appear here, because
/// an editor resolves by the first and this crate by the second.
const ALIASES: [(&str, &str); 15] = [
    ("json", "json"),
    ("jsonc", "json"),
    ("yaml", "yaml"),
    ("yml", "yaml"),
    ("csv", "csv"),
    ("tsv", "csv"),
    ("toml", "toml"),
    ("ini", "ini"),
    ("cfg", "ini"),
    ("conf", "ini"),
    ("properties", "ini"),
    ("env", "env"),
    ("dotenv", "env"),
    ("text", "text"),
    ("txt", "text"),
];

/// The formats a caller can name, for the tool schema's enum. Held equal
/// to the alias table by a test, so a format can never be offered and
/// then not resolve.
pub(crate) const SUPPORTED_FORMATS: [&str; 7] =
    ["json", "yaml", "csv", "toml", "ini", "env", "text"];

/// What the engine uses when it recognises nothing.
///
/// **`text`, not `unknown`.** Unlike the sibling crates, falling through
/// here is not a degraded mode — the scan finds exactly the same
/// identifiers it finds in a JSON file, and only the key path is missing.
/// Naming it `unknown` would tell a reader the document was not
/// understood, when what actually happened is that it had no keys to
/// report.
pub(crate) const FALLBACK_FORMAT: &str = "text";

fn normalise(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .trim_start_matches('.')
        .to_string()
}

/// The reader key for an already-canonical format name, or the fallback.
/// Used on the hot path, where the caller has resolved once.
pub(crate) fn canonical(format: &str) -> &'static str {
    ALIASES
        .iter()
        .find(|(alias, _)| *alias == format)
        .map_or(FALLBACK_FORMAT, |(_, key)| *key)
}

/// Resolve a reader key from an explicit format, else from a filename,
/// else the fallback.
pub(crate) fn resolve_format(format: Option<&str>, filename: Option<&str>) -> &'static str {
    if let Some(name) = format {
        let direct = canonical(&normalise(name));
        if direct != FALLBACK_FORMAT {
            return direct;
        }
    }

    let Some(filename) = filename else {
        return FALLBACK_FORMAT;
    };

    // A dotfile like `.env` has no extension to split on; its whole name
    // is the type.
    let whole = canonical(&normalise(filename));
    if whole != FALLBACK_FORMAT {
        return whole;
    }

    filename
        .rsplit_once('.')
        .map_or(FALLBACK_FORMAT, |(_, extension)| {
            canonical(&normalise(extension))
        })
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
            ("tsv", "csv"),
            ("cfg", "ini"),
            ("conf", "ini"),
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
        for name in ["markdown", "dockerfile", "", "wat"] {
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
                ALIASES.iter().any(|(_, key)| *key == format),
                "{format} is offered but no alias produces it"
            );
        }
        for (_, key) in ALIASES {
            assert!(
                SUPPORTED_FORMATS.contains(&key),
                "{key} is produced but not offered"
            );
        }
    }
}
