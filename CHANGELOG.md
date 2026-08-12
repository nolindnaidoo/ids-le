# Changelog

All notable changes to ids-le are documented here. The crate keeps its own
[`crate/CHANGELOG.md`](crate/CHANGELOG.md) with the release detail; this file
is the repository's view.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **An ObjectId is named only where the document names the field an
  identifier** — a key path whose leaf ends in `id`, plus a plausible
  embedded timestamp. An ObjectId's whole specification is *24 hex
  characters*, so every truncated SHA-1 and MD5 digest is a structurally
  perfect one; the timestamp alone named 163 of 600 ordinary digests, and
  the key requirement takes that to 0. It is the rule Snowflake has
  always applied, extended to the other kind with nothing to validate,
  and the two now share one predicate. See
  [`crate/CHANGELOG.md`](crate/CHANGELOG.md) for the full note.

## [0.1.0]

First release. Core functionality; not yet published to crates.io.

### Added

- **The `ids-le` CLI and MCP server**, in [`crate/`](crate/). Five identifier
  kinds — `uuid`, `ulid`, `nanoid`, `objectid`, `snowflake` — each with its
  structure checked rather than its shape matched, and every embedded
  timestamp decoded to an ISO-8601 UTC string across six unrelated bit
  layouts and three epochs.

- **Refusals as first-class rows.** A run this crate will not name is a row
  carrying `valid: false`, a named reason, a sentence saying why, and
  whatever was decoded before the refusal. Five reasons: `ambiguous_kind`,
  `malformed`, `nil_or_max`, `version_claim_mismatch`,
  `timestamp_implausible`.

- **Key paths from six formats** — JSON/JSONC, YAML, TOML, INI, dotenv and
  CSV — with everything else read as text. The format changes only how a
  finding is addressed, never which runs are found.

- **Exit codes that follow grep**: 0 found, 1 none found, 2 malformed
  question. A refusal does not move them unless `--strict`.

- **Repository documentation**: this file, [README.md](README.md),
  [AGENTS.md](AGENTS.md) and the MIT [LICENSE](LICENSE).

See [`crate/CHANGELOG.md`](crate/CHANGELOG.md) for the full release note,
including the decisions worth knowing before reading the code.
