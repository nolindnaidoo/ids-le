# ids-le (CLI) — engineering standards

This is the source of truth for how code in `crate/` is written, tested,
and reviewed. It applies to every contributor, human or AI-assisted.
[SPEC.md](SPEC.md) defines the product behaviour — kinds, refusals, exit
codes, both surfaces; this file is how the code gets there. AGENTS.md
wins on any conflict about *how*.

## What this project is

Get every identifier out of a codebase, say what it is and when it was
minted, and **refuse honestly** where it cannot be named. UUID, ULID,
NanoID, ObjectId, Snowflake.

**The refusal is the product.** Everything else here could be rebuilt
from a regex and a bit shift. Deciding that a run of characters *cannot*
be named — and saying which two schemes fit and what evidence was missing
— is what a regex cannot do, and it is the reason this exists rather than
a shell alias. Every design argument below resolves in favour of saying
less, more precisely.

**The reader is not the author.** Someone tracing which service minted a
record, or auditing what identifiers a repository leaks, is usually
without a checkout and always without the editor open. That is why the
report carries a key path, why columns are UTF-16, and why a timestamp is
ISO-8601 rather than an epoch integer.

**Status: 0.1.0, core functionality.** Every kind, every refusal reason,
both surfaces and the test layers below are built and green. It is not
hardened: see "Deliberately left" at the end.

## Layout

```
crate/src/
├── extract/     pure: the candidate scanner, the five kinds, the
│                classification policy, key paths, positions, time.
│                No filesystem, no clock.
├── walk.rs      ignore-aware tree walking
├── scan.rs      one file end to end — the only path either surface
│                calls, and the only place the wall clock is read
├── cli.rs       the terminal surface
└── mcp/         the agent surface
```

Inside `extract/`:

| Module | Holds |
|---|---|
| `candidate.rs` | Maximal runs over `[0-9A-Za-z_-]`, with offsets. One scanner, every format. |
| `policy.rs` | `Kind`, `Reason`, `Verdict`, the `Clock`, and the length router that sends a candidate to its kind. |
| `uuid.rs` `ulid.rs` `nanoid.rs` `objectid.rs` `snowflake.rs` | One scheme each: its structure, its decode, its refusals. |
| `time.rs` | Epoch milliseconds → ISO-8601 UTC. Hand-written civil-date arithmetic. |
| `locate.rs` | `KeySpan`, the offset → key-path lookup, and the shared line iterator. |
| `json.rs` `yaml.rs` `toml.rs` `ini.rs` `dotenv.rs` `csv.rs` | Key paths only. Which byte ranges are values, and what names them. |
| `position.rs` | Byte offset → line/UTF-16 column, over a checkpoint index. The checkpoints are what keep a minified line linear; see the note there. |
| `format.rs` | Which key-path reader a document gets. |
| `corpus.rs` | `#[cfg(test)]`. The pinned fixtures. |

- **`extract/` touches no filesystem and reads no clock.** Both would
  make the analysis untestable in the same way: a plausibility window
  that moves with the wall clock fails on a future Tuesday. `scan.rs`
  reads the clock and hands down a `Clock`. A `std::fs` or
  `SystemTime::now` in `extract/` is a bug.
- **Both surfaces are one implementation.** `cli.rs` and `mcp/` both call
  `scan.rs`. A surface that grows its own copy of a rule is a bug, and a
  contract test asserts the two return identical reports for the same
  tree.
- **`walk.rs` selects, it does not decide.** Its one rule — a file named
  explicitly is read whatever the ignore rules say — is why intent beats
  configuration.
- Keep modules flat. No layers, registries, managers, or services. No
  trait with a single implementation.

## Decisions already made (do not relitigate)

- **The format decides how a finding is *addressed*, never whether it is
  a finding.** This is the structural inversion of `numbers-le`, where
  the format decides what counts (`"42"` is a string in JSON and a number
  in `.env`). An identifier has no such fork, so one candidate scanner
  runs over every document and the readers in `extract/*.rs` supply key
  paths and nothing else. A reader that starts filtering findings has
  broken the design.
- **Positions are never lost.** A number's printed form differs from its
  source text, which is why `numbers-le` needs a search-by-value and
  carries an `unlocated` count. An identifier's raw text *is* its value,
  so the scan that finds it already knows where it is. There is no
  `unlocated` field here and there must not be one.
- **A candidate is a *maximal* run.** `user_550e8400-…` is one run, not a
  prefix beside a UUID. Stripping a prefix this tool cannot verify is
  guessing which part of a string is the identifier.
- **The key path is evidence, not decoration.** Four kinds ask it whether
  a run is what its shape suggests. A reader that mislabels a value can
  turn a refusal into a finding, which is why each one states its limits
  in its own module doc.
- **A run with no structure to check is named only where the document
  names the field an identifier.** Snowflake and ObjectId are the two:
  a large integer and 24 hex digits, neither carrying a version, a
  variant, a checksum or a restricted alphabet. Every 24-hex run is a
  structurally perfect ObjectId — a truncated SHA-1 included — so the
  shape can never settle it and the timestamp alone admitted better than
  one hash in four. They share **one** predicate, `policy::names_an_id`;
  a second definition of "the document names this an identifier" would
  drift from the first. The kinds that *do* have structure — UUID's
  nibbles, ULID's alphabet and bounded leading character, NanoID's
  base64url — are checked on their own terms and do not consult it.
- **`--kind` is a view, applied after the analysis.** A refusal that
  named no kind disappears under any `--kind`, so the unfiltered run is
  the complete one. SPEC.md says so; do not "fix" it by filtering
  earlier, which would change which runs are analysed.
- **An unknown format falls back; an unknown kind is refused.** The
  asymmetry is deliberate and tested on both surfaces: a bad format costs
  key paths, and a bad kind would return an empty report that a caller
  reads as "this tree is clean".
- **stdout is protocol, stderr is human. There is no `--json` flag.** One
  mode, nothing to misremember, and the human summary is a projection of
  the same report so the two cannot drift.
- **No dependency per identifier kind, and no date library.** `serde`,
  `serde_json` and `ignore` are the whole tree. `uuid` was considered for
  the version/variant bits and rejected: those are two nibbles, and the
  crate refuses to construct anything it considers invalid — but refusing
  is *this* tool's job, and a refusal arriving as `Err(())` cannot carry
  the named reason the whole report is built on. The reasoning is in
  Cargo.toml where someone adding a dependency will read it.
- **`overflow-checks` stays on in release.** Every output is a claim
  someone scripts against — a line number, a column, a decoded instant. A
  wrapped number is silently wrong data in a report whose whole value is
  honesty, which is worse than a crash.

## Control-flow style

Flat over nested, guards over branches — the same rules as pixelcoords,
pixelactions and scrape-le:

- **No statement-position `else`.** Guard clauses and early `return`
  (`if !ok { return ... }` / `let Some(x) = ... else { return }`), then
  fall through to the happy path.
- **Value-position `if/else` is fine** — `let x = if cond { a } else
  { b }` is Rust's ternary.
- **`match` is fine and preferred** over any chain of condition tests on
  the same value; use match guards instead of `if/else` inside arms.
- Prefer combinators where they read cleanly: `bool::then_some`,
  `Option::map/filter/is_some_and`, `?`.
- No nesting deeper than two levels inside a function; extract a named
  helper instead.

## Hard rules

- **No inline `#[allow(...)]`.** Either fix the lint or add a visible,
  commented relaxation to `[lints.clippy]` in `Cargo.toml`. Four are
  there, each with its reason.
- **Clippy pedantic, deny warnings.** `cargo clippy --all-targets --
  -D warnings` must pass.
- **`unsafe` is forbidden crate-wide** (`[lints.rust]`).
- **No `anyhow`, no `thiserror` in the library.** `Result<T, String>` is
  the error type; the message *is* the documentation.
- **Dependencies are a cost.** Three is already a position. Justify every
  addition in a Cargo.toml comment; prefer the standard library.
- **No network, ever.** Nothing here verifies an identifier against a
  database or an API, and no future feature may.
- **Read-only.** The filesystem is never written outside tests.
- **Strict parsing, never silent defaults.** A bad flag is an error with
  an actionable message. The one sanctioned leniency is an unrecognised
  *format*, and it is tested.
- **Refuse rather than guess.** A run that fits two schemes is refused
  with both named. A refusal carries a reason and the evidence behind it
  — including the decode that caused it, where there was one. Never a
  dropped row; never a silent success.
- **Refusals speak the caller's vocabulary.** An MCP caller has no
  command line; no message on that surface mentions a flag, and a test
  greps for `--` across every tool definition and failure path.

## Testing

- **`extract/` is pure, so it tests from a string.** No temp directories,
  no clocks, no flake. Every rule in a kind module has a test naming the
  rule, and every refusal has a test asserting the *reason* rather than
  just that something was refused.
- **The corpus is the composition test.** `fixtures/extraction.json` pins
  whole documents through every reader; `ambiguous.json` pins twelve runs
  that must all be refused, with each reason asserted; `timestamps.json`
  pins six decodes across four schemes. A change that keeps every unit
  test true and still moves the report fails here.
- **The corpus runs against a pinned clock** (`CORPUS_CLOCK`,
  2026-08-12), because the plausibility window ends a year past now and a
  corpus read against the wall clock would be a test of the calendar.
- **Decodes are pinned against values computed outside this
  implementation.** Three are RFC 9562's own examples, which the RFC
  states name one instant — three different bit layouts landing on one
  timestamp is a check the implementation cannot fake. A new decoder
  arrives with an externally-computed value, never with whatever the code
  currently prints.
- **Exit codes belong in `tests/contracts.rs`.** They are the API —
  callers branch on them — so they are pinned by tests that drive the
  built binary. **A new refusal adds its case there**, and
  `every_refusal_reason_reaches_the_report` asserts the whole set.
- **Volume belongs in `tests/scenarios.rs`**, gated behind
  `IDS_LE_SCENARIOS`. A skipped scenario is never reported as a pass.
- **Every bug fix ships with a regression test** that fails before the
  fix.
- Tests are deterministic: no wall clock, no randomness, and no network.

Five suites beyond those, each with its own CI job:

| Suite | Holds | Gate |
|---|---|---|
| `tests/hazards.rs` | What a real machine holds and a fixture directory cannot: a BOM, invalid UTF-8, UTF-16, a FIFO, a mode-000 file, a symlink loop, a 300-character path, an empty file, a multi-megabyte minified line, a 3 MB base64 blob. The tree is built at runtime and a case the platform cannot express **is skipped by name**. | 3-OS matrix |
| `tests/platform.rs` | Forward slashes in every reported path, case folding, reserved Windows names, CRLF, stdin — and that no decode moves with the machine's time zone. The job runs the whole suite under `TZ=UTC`, under none, and under `Pacific/Kiritimati`. | 3-OS matrix |
| `tests/fuzz.rs` | Generated runs at every boundary the classifier decides on, time-boxed by `IDS_LE_FUZZ_SECONDS` and seeded from `IDS_LE_FUZZ_SEED`, both printed. Asserts no panic, no stall, a well-formed row — and **never a kind named where two schemes fit**. | 60 s in CI |
| `tests/budget.rs` | A wall-clock ceiling on a generated 500-file corpus, plus linearity across files, across identifiers in one file, and across identifiers on one non-ASCII line. Gated behind `IDS_LE_BUDGET`; the measurement and the machine it came from are in the module doc. | release, `--test-threads=1` |
| `tests/coverage_matrix.rs` | Every kind, reason, UUID version, variant and format reachable from a real fixture — with `Kind`, `Reason`, `Variant` and `SUPPORTED_FORMATS` read out of `src/extract/` rather than typed into the test. Also asserts that **no** hash in prose is named an ObjectId, and prints the residual rate under an id-naming key. | greps its own marker |

- **The matrix prints `coverage-matrix: complete` and CI greps for it.**
  `cargo test <filter>` exits 0 when the filter matches nothing, so a
  renamed or deleted test would otherwise leave a green job that asserted
  nothing. Do not change the string without changing the job.
- **A timing number in a module doc names the machine it came from.** If a
  ceiling is tight on a runner, re-measure there and record it — never
  quietly raise the number.

## Verification — the definition of done

All of it, before every push:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
IDS_LE_SCENARIOS=1 cargo test --release --test scenarios
IDS_LE_FUZZ_SECONDS=60 cargo test --release --test fuzz -- --nocapture
IDS_LE_BUDGET=1 cargo test --release --test budget -- --nocapture --test-threads=1
```

`cargo test --locked` already runs `hazards`, `platform` and
`coverage_matrix`; add `-- --nocapture` to see what a platform skipped and
what the matrix measured.

**Run the binary, not only the tests.** Point it at `fixtures/documents/`
and read the output. The scenario suite caught a generated fixture whose
all-zero tail tripped `version_claim_mismatch` — a real rule firing
correctly on a test's mistake, and nothing but a run would have shown it.

A change is not done because it compiles; it is done when it is tested,
linted, documented where behaviour changed (README / SPEC / CHANGELOG /
this file), and honest — claims in docs must match the code.

## Deliberately left for 0.2.0

Named here so nobody mistakes them for oversights:

- **Not published.** `cargo install ids-le` does not work yet; the README
  says so plainly rather than printing a line that fails. Publication is a
  release decision, not a code one.
- **No VS Code extension beside it**, so unlike the siblings there is no
  parity corpus and no second implementation to be held equal to.
  `fixtures/extraction.json` is a characterisation record of this crate's
  own behaviour; its job is that a change to the report is deliberate and
  visible in a diff.
- **The YAML and TOML readers are line scanners.** Multi-line arrays,
  flow style, anchors and multi-document files cost key paths — never
  findings. Their limits are in SPEC.md and in each module's doc.
- **The plausibility window is fixed** at 1990 and now + one year. It is
  not configurable, and making it so would need a reason better than
  "someone might want to".
