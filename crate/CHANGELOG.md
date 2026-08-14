# Changelog

The Rust CLI and MCP server.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-08-14

**0.2.0, not 0.1.1.** Two of the changes below narrow what this crate
names, so a run against the same tree returns fewer findings than 0.1.0
did. That is a breaking change to the answer, and 0.x spends the minor
on it.

### Changed

- **One sentence describes this crate everywhere it is described.** The
  `description` in `Cargo.toml`, the line under the title in
  `README.md`, and the entry on letools.dev had drifted into three
  paraphrases, so the crate a reader met on crates.io was not obviously
  the one they met on the site. Nothing about the tool moved.

- **Which reader a document gets is a type, not a string.** `key_spans`
  dispatched on `&str` with a `_ => Vec::new()` catch-all, so a reader
  added to `format.rs` and not wired up in `locate.rs` would have
  silently cost every key path in that format — and a key path is
  evidence, so it would have cost verdicts too. Three tests noticed that
  class of drift between them; `Reader` makes it two compile errors. No
  name on the wire moves: `Reader::name` is the single place a reader is
  spelled, and `FALLBACK_FORMAT` is defined from it.

- **Every kind reads the same part of the key path to decide what a field
  is.** ULID and NanoID asked whether the *whole* path mentioned their
  scheme, while ObjectId and Snowflake asked whether the *leaf* did — so
  one document answered two ways about the same question:

  ```yaml
  ulids:
    count: 01kzsm9k00abcdefgh12345678   # was: named a ulid
  snowflakes:
    count: 1536886938009600000          # correctly: not a finding
  ```

  A count that happens to sit in a table about identifiers is still a
  count, and `policy.rs` had said so all along — only two of the four
  kinds followed it. The three schemes that can be named outright now
  share one predicate, `policy::names_scheme`, which reads the leaf, next
  to `policy::names_an_id`, which already did.

  **One exception, and it is written down rather than left to be
  rediscovered.** Snowflake still reads the whole path to choose between
  the Twitter and Discord epochs, because which *platform* minted an
  identifier is a fact about the structure it hangs off — `discord.id` is
  a Discord id — where which *scheme* a field holds is a fact about the
  field. `policy::key_mentions` now exists for that one use and says so.

### Fixed

- **`ids_le_scan`'s tool schema is as strict as `extract_ids`'s.** It
  declared neither `required` nor `additionalProperties: false`, so a
  model that misspelled `hidden` was told nothing and got a walk it had
  not asked for. The schema now sets `additionalProperties: false` and
  `anyOf: [{required: ["path"]}, {required: ["paths"]}]`, which is what
  the handler has always enforced in prose.

  The tool moved to `src/mcp/scan.rs`, beside `extract.rs`, because that
  is why the two drifted: `extract_ids` kept its schema next to its
  handler and `ids_le_scan` had its schema in the transport module a
  hundred lines from the code honouring it. `mod.rs` is the transport and
  the envelope; a tool is a module. A test now holds every tool's schema
  to both rules, and another turns every knob `ids_le_scan` advertises —
  `hidden` and `ignored` had no test on this surface at all.

- **A year outside `0000`–`9999` is written in ISO-8601's expanded form.**
  The report calls every decoded instant an ISO-8601 UTC string, and a
  five-digit year printed as bare digits is not one — the standard asks
  for a sign and an agreed number of extra digits. This crate agrees on
  six, matching `Date.toISOString`, so a ULID at the top of its 48-bit
  field now reads `+010889-08-02T05:31:50.655Z` rather than
  `10889-08-02T05:31:50.655Z`.

  Only a refusal ever carries one: the plausibility window closes a year
  past now, so any such decode arrives as `timestamp_implausible` with
  the instant beside it as the evidence a reader is being asked to check.
  The largest instant any kind here can decode is 10889 — a ULID or a
  UUID v7 whose millisecond field is all ones. Nothing a real document
  holds is affected, and the corpus does not move.

- **SPEC.md no longer documents an exit code that cannot happen.** The
  exit-code table said `2` covered "the question was malformed, **or a
  scan gave up part way**", and `exit_code` opened with a branch for the
  second half. Nothing could produce it: `Diagnostic.severity` was a
  `String`, the branch tested it for `"error"`, and the only diagnostic
  this crate constructs is the `warning` for a file it could not read.
  A file is read whole or the reason it was not is named — there is no
  partial state, so the claim is gone and so is the branch.

  `Severity` and `Code` are enums now rather than strings, each with one
  variant and the reason for that written on it. The wire format is
  unchanged (`"severity": "warning"`, `"code": "skipped"`), and a new
  variant no longer compiles until something produces it and a test names
  it. The MCP envelope's `ok` was computed the same way against the same
  impossible severity; it is stated as the constant it always was, with
  the reason — a tool that could not run returns `isError` and never
  reaches an envelope.

- **A trailing comment no longer lends its line's key path to the runs
  inside it.** The TOML, INI and YAML readers ran a value region to the
  end of the line, so everything after a `#` or `;` was addressed as
  though it were the value. The key path is *evidence* — four of the five
  kinds ask it whether a run is what its shape suggests — so this
  promoted refusals to findings:

  ```toml
  _id = 1 # 6a7bb780a1b2c3d4e5f60718   # was: objectid, key "_id"
  ```

  ```dotenv
  _ID=1 # 6a7bb780a1b2c3d4e5f60718     # was, correctly: refused
  ```

  Four readers of one document, and only `dotenv.rs` had the rule, which
  is how the disagreement surfaced. The rule is now written once, in
  `locate::value_length`, and each reader passes its own comment
  characters.

  It is **quote-aware**, because that is the whole reason quoting exists:
  `id = "a # b"` is a value carrying a hash, and a backslash hides the
  closing quote of a double-quoted string. It also requires whitespace in
  front of the comment character, which is the conservative direction —
  `a: b#c` stays one YAML plain scalar and `A=#x` stays a dotenv value,
  so the change can leave a comment attached but can never eat a value.

  Two smaller consequences fall out of the same rule. A dotenv value that
  was quoted used to take the whole rest of the line, so a comment after
  it borrowed the key; it no longer does. And a YAML key whose line holds
  nothing but a comment (`a: # note`) introduces a block rather than
  carrying a value, which it already meant.

  `fixtures/documents/comments.toml` pins it as a document: the same 24
  hex characters as a value under a key naming an id, inside a quoted
  value under one, and in a trailing comment on a line whose key names
  one — named, named, refused.

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

[0.1.0]: https://crates.io/crates/ids-le/0.1.0
[0.2.0]: https://crates.io/crates/ids-le/0.2.0
