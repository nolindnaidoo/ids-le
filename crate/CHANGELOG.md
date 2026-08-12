# Changelog

The Rust CLI and MCP server.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **An ObjectId is named only where the document names the field an
  identifier.** A 24-hex run is now named when the leaf of its key path
  ends in `id` — `_id`, `userId`, `$oid`, `objectId` — **and** its
  leading four bytes decode to a plausible time. Either signal missing is
  `ambiguous_kind`, carrying the decode as before.

  An ObjectId's entire specification is *24 hex characters*: no version,
  no variant, no checksum, no reserved bits. Every 24-hex run is
  therefore a structurally perfect ObjectId, and so is every truncated
  SHA-1 and MD5 digest — the run can never settle it, because there is
  nothing in it to settle it with. The timestamp alone was admitting
  **163 of 600** ordinary abbreviated digests (27.2%, and the plausible
  window covers 27.6% of the 32-bit second space, so that is the
  expected rate rather than bad luck). It is now **0 of 600**.

  This is not a new rule. It is the rule `snowflake.rs` has always
  applied — a bare integer is not a Snowflake without a key naming an id
  — extended to the other structureless kind, and the two now share one
  predicate, `policy::names_an_id`, rather than two that could drift.
  UUID, ULID and NanoID are unchanged: each has structure of its own and
  is checked on its own terms.

  **The trade is recall.** A genuine ObjectId in prose, or under a field
  called `checksum`, is now refused — with its position, its decode and a
  reason, so a reader can disagree. The residual is stated too: under a
  key that does name an id, the timestamp is the only remaining filter,
  so roughly 27% of digests stored in a field called `commitId` are still
  named. `tests/coverage_matrix.rs` asserts the first number is zero and
  prints both on every run.

  `fixtures/documents/context.json` is new and pins the rule as a
  document: the same 24 characters four times, named under `_id` and
  `documentId`, refused under `checksum` and in an array — and refused
  all four times when the same bytes are read as text.

### Fixed

- **Reported paths use `/` on every platform.** On Windows the report
  carried whatever separator the filesystem handed back, so the same tree
  scanned on Windows and on Linux produced two different reports for no
  reason a reader could see. A report is diffed against one produced
  somewhere else; that is most of what a report in CI is for.

- **A minified document carrying a non-ASCII character is no longer
  quadratic.** Column lookup counts UTF-16 code units, and a document
  with any non-ASCII byte in it took the counting path — which re-counted
  from the start of the line on every identifier. On one very long line
  that is a square: 10,000 identifiers took 1.11 s and 20,000 took 4.03 s
  on the release binary. `extract/position.rs` now carries a checkpoint
  every kilobyte, and the same documents take 0.03 s and 0.05 s. The
  answers are unchanged, and a test asserts the indexed path agrees with
  the counted one at every offset.

### Added

- **Five test suites**, each with its own CI job: `hazards` (a
  runtime-built tree of a BOM, invalid UTF-8, UTF-16, a FIFO, a mode-000
  file, a symlink loop, a 300-character path, a multi-megabyte minified line and a
  3 MB base64 blob, on three platforms), `platform` (path separators,
  case folding, reserved names, CRLF, stdin, and the whole suite under
  three time zones), `fuzz` (generated runs at every boundary the
  classifier decides on, time-boxed and seeded), `budget` (a wall-clock
  ceiling plus three linearity checks) and `coverage_matrix` (every kind,
  reason, UUID version, variant and format reachable from a real
  fixture).

- **`fixtures/documents/versions.json`** — one UUID of every version
  RFC 9562 defines and one of every variant, so "all versions" is a claim
  with a document behind it. The corpus previously held v1, v4, v6 and
  v7.

- **`fixtures/documents/hashes.txt`** — 600 abbreviated SHA-1 and MD5
  digests, and the measurement they exist for: the share of ordinary
  24-hex runs named as ObjectIds. It measured 27.2% (163 of 600) against
  a plausibility window covering 27.6% of the 32-bit second space, which
  is what the rule change above was decided on.

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
