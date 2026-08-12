# Changelog

The Rust CLI and MCP server.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0]

First release. Core functionality; not yet hardened.

### Added

- **Five identifier kinds**, each with its structure checked rather than
  its shape matched: `uuid` (all versions, with `version` and `variant`),
  `ulid`, `nanoid`, `objectid`, `snowflake`.

- **Timestamp decoding, uniform across six unrelated bit layouts** —
  UUID v1 and v6 (60 bits of 100-nanosecond intervals since the Gregorian
  reform in 1582, in two different field orders), UUID v7 (48 bits of
  Unix milliseconds), ULID (48 bits of Crockford base32), ObjectId (32
  bits of Unix seconds), and Snowflake (42 bits over the Twitter or
  Discord epoch). Every one reported as an ISO-8601 UTC string. The
  corpus pins the RFC 9562 v1, v6 and v7 examples, which the RFC states
  name one instant.

- **Refusals as first-class rows.** A run this crate will not name is a
  row carrying `valid: false`, a named `refused` reason, a `detail`
  sentence, and whatever was decoded before the refusal. Five reasons:
  `ambiguous_kind`, `malformed`, `nil_or_max`, `version_claim_mismatch`,
  `timestamp_implausible`. See SPEC.md for the table and the boundaries.

- **Key paths from six formats** — JSON/JSONC, YAML, TOML, INI (`.cfg`,
  `.conf`, `.properties`), dotenv and CSV — with everything else read as
  text. The format changes only how a finding is addressed, never which
  runs are found.

- **The CLI**: `ids-le [options] <file|dir>...`, `--stdin`, `--format`,
  `--kind`, `--strict`, `--hidden`, `--no-ignore`. stdout is one JSON
  report per line (`schema: 1`); stderr is the human projection of the
  same reports. Exit codes follow grep — 0 found, 1 none, 2 malformed
  question — and a refusal does not move them unless `--strict`.

- **The MCP server**: `ids-le mcp`, protocol `2025-06-18`, offering
  `extract_ids` (content, no filesystem) and `ids_le_scan` (paths). Both
  return `{ ok, data, diagnostics, meta }`.

- **A pinned corpus** — nine documents in `fixtures/`, including an
  ambiguity set where every one of twelve runs is refused with its reason
  asserted, and a timestamp set pinning six decodes across four schemes.

### Decisions worth knowing

- **A bare integer is never a Snowflake without a key that names an id.**
  Nothing in an 18-digit integer says it is an identifier. In a
  plain-text file, where there are no keys, no integer is one.
- **Only the canonical hyphenated form is a UUID.** The 32-character
  unhyphenated form is refused as `ambiguous_kind` — it is
  character-for-character an MD5 digest.
- **No `--json` flag**, matching the rest of the family: one mode, and
  the human summary is a projection of the same report.
- **No dependency per identifier kind, and no date library.** `serde`,
  `serde_json` and ripgrep's `ignore` are the whole tree; the Cargo.toml
  says why `uuid` was considered and rejected.
