# Changelog

All notable changes to IDs-LE will be documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This file covers the **VS Code extension**. The Rust CLI in `crate/` is a
separate product on its own cadence and keeps its own
[CHANGELOG](crate/CHANGELOG.md). The entries below 1.0.0 describe this
repository while it held the CLI alone.

## [1.1.0] - 2026-10-06

### Added

- Scan a folder or the whole workspace. `IDs-LE: Scan Workspace for IDs`
  reads every file in the workspace from disk. `IDs-LE: Scan Folder for IDs`
  does the same for one folder, from the command palette or from a folder in
  the Explorer. Files are read from disk, so an unsaved edit is not seen. The
  report opens with a table of every file that holds an identifier, then has
  a section per file, and ends with a line for each thing the scan left
  unread.
- A scan skips what the project's `.gitignore` files skip, as well as
  `node_modules`, build output, caches and lockfiles.
  `ids-le.workspace.scanRespectGitignore`, `ids-le.workspace.scanPatterns`
  and `ids-le.workspace.scanExcludes` change which files are read.
- `ids-le.workspace.scanMaxFiles` caps how many files are read and
  `ids-le.workspace.scanMaxResults` caps how many identifiers are listed. A
  file over the safety size, or one that is not UTF-8 text, is left unread,
  and the report says how many were.
- Runs that could not be named are counted per file in a scan, not listed.
  `ids-le.workspace.scanIncludeRefusals` lists each one, and
  `ids-le.workspace.scanProblemsEnabled` also shows them in the Problems
  panel. Both are off by default: across a project they run to thousands,
  mostly digests in generated files.
- The positions settings apply to a scan as they do to Extract.
- Positions are now a setting. `ids-le.showPositions` decides whether the
  report gives the line and column of each identifier, and
  `ids-le.clipboardIncludesPositions` decides the same for the copy on the
  clipboard. Both are on by default, so the report is what it was. With
  `showPositions` off, a row that read `**3:13** · f47ac10b-…` reads
  `f47ac10b-…`, and nothing else about the report changes.

### Changed

- No command is bound to a key by default any more. The one default this
  extension shipped sat on a key the editor, the system or another LE
  extension already used. Every command can still be given a key under
  Keyboard Shortcuts.

## [1.0.1] - 2026-10-04

### Fixed

- The Open VSX links and the Open VSX downloads badge in the README and the
  npm README pointed at a namespace the listing has left, so they led nowhere.
  The listing is under `nolindnaidoo` now, and so are they.

### Removed

- The Zed extension in `zed/`, with the CI job that built it and the workflow
  that synced it. It was never listed in Zed's registry.

## [1.0.0] - 2026-10-03

### Added

- **The VS Code extension.** `IDs-LE: Extract IDs` lists every UUID, ULID,
  NanoID, MongoDB ObjectId and Snowflake in the active document by kind, with
  its line and column, its key path, its validity and the instant it was
  minted where it carries one, and reports every run it could not name with
  the reason. `ids-le.kind` narrows the named rows; refusals are always shown.
- **The MCP server in the VSIX and on npm** as `ids-le-mcp`: the same
  `extract_ids` tool the Rust CLI serves, answering identically.
- **The engine is a port of the crate's**, held to it by the shared corpus, a
  differential that feeds both servers thousands of generated documents in
  every format, and a check that both servers define the tool identically.
- Localized into twelve languages: the manifest and every runtime string.
- A Zed extension that runs the MCP server as a context server.

## [0.2.0] - 2026-08-14

Breaking, and 0.x, so the next release is **0.2.0** rather than a patch:
two of the changes below narrow what gets named.

### Added

- **A terminal demo** at [`assets/demo.gif`](assets/demo.gif), driving
  the real binary over the files in [`assets/demo/`](assets/demo/).
  [`assets/demo.tape`](assets/demo.tape) is the `vhs` script that
  produced it, so `cd assets && vhs demo.tape` reproduces the recording
  rather than leaving an artifact nobody can regenerate. Both sit above
  `crate/`, where `cargo package` cannot reach them.

### Changed

- **New icon artwork.** All sixteen tools were redrawn in one style, so
  the family reads as one set wherever the cards sit side by side. The
  framing is unchanged — the drawing fills 65.8% of an 800×800 canvas
  and every smaller size is derived from that one file rather than drawn
  again.

- **An ObjectId is named only where the document names the field an
  identifier** — a key path whose leaf ends in `id`, plus a plausible
  embedded timestamp. An ObjectId's whole specification is *24 hex
  characters*, so every truncated SHA-1 and MD5 digest is a structurally
  perfect one; the timestamp alone named 163 of 600 ordinary digests, and
  the key requirement takes that to 0. It is the rule Snowflake has
  always applied, extended to the other kind with nothing to validate,
  and the two now share one predicate. See
  [`crate/CHANGELOG.md`](crate/CHANGELOG.md) for the full note.

### Fixed

- **The README's images resolve away from GitHub.** They were repository
  paths, which crates.io and every other renderer resolves against its
  own origin, so the demo and the icon were broken everywhere this file
  is read that is not this repository. They are absolute URLs now.

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

[0.1.0]: https://crates.io/crates/ids-le/0.1.0
[0.2.0]: https://crates.io/crates/ids-le/0.2.0
