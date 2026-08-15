# AGENTS.md — ids-le

Technical source of truth for this repository. [README.md](README.md) is the
user-facing page; this file is for anyone, human or agent, changing the code.

**This repo is crate-only.** Everything that runs lives in
[`crate/`](crate/), and the standard it is held to is
[`crate/AGENTS.md`](crate/AGENTS.md) — layout, control-flow style, the
settled decisions, the testing requirements and the definition of done. Read
it before writing a line. [`crate/SPEC.md`](crate/SPEC.md) defines the
product behaviour — kinds, refusals, exit codes, both surfaces. AGENTS.md
wins on any conflict about *how*; SPEC.md wins about *what*.

Unlike most of the LE family there is no VS Code extension at the root yet.
When one arrives it is a second product with its own document; nothing in
`crate/` bends to accommodate it in advance.

## Where to look

| Question | File |
|---|---|
| How is code in this repo written? | [`crate/AGENTS.md`](crate/AGENTS.md) |
| What does the tool do, exactly? | [`crate/SPEC.md`](crate/SPEC.md) |
| What does a user see? | [README.md](README.md) |
| What changed? | [CHANGELOG.md](CHANGELOG.md) |

## Layout

```
crate/          the Rust CLI and MCP server — the whole product
  src/          extract/ (pure), walk.rs, scan.rs, cli.rs, mcp/
  tests/        contracts, scenarios, hazards, platform, fuzz, budget,
                coverage_matrix
  fixtures/     the pinned corpus, shared by the unit tests and the matrix
.github/        CI, CodeQL, Dependabot
```

## Gates

Exactly what CI runs, from `crate/`:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

The suites that are gated off a laptop run in their own CI jobs, and each is
a real gate rather than a report:

| Job | Command | Catches |
|---|---|---|
| `hazards` | `cargo test --test hazards -- --nocapture` | A file a real machine holds and a fixture directory cannot: a BOM, invalid UTF-8, UTF-16, a FIFO, a mode-000 file, a symlink loop, a 300-character path, a multi-megabyte minified line |
| `platform` | `cargo test --test platform -- --nocapture`, then the suite under three time zones | Separators, case folding, reserved names, CRLF, stdin — and that no decode moves with the machine's zone |
| `fuzz` | `IDS_LE_FUZZ_SECONDS=60 cargo test --release --test fuzz` | A panic, a stall, or a kind named where two schemes fit |
| `budget` | `IDS_LE_BUDGET=1 cargo test --release --test budget -- --test-threads=1` | An order-of-magnitude slowdown, and anything quadratic |
| `coverage-matrix` | `cargo test --test coverage_matrix -- --nocapture`, then a grep | A kind, reason, version, variant or format with no fixture behind it |
| `coverage` | `cargo llvm-cov` | Any module in `extract/` under a 75% line floor |

**The `coverage-matrix` job greps for a marker line**, and that is not
decoration: `cargo test <filter>` exits 0 when the filter matches nothing, so
a renamed or deleted test would otherwise leave a green job that asserted
nothing at all.

## Things that will bite you

- **Coverage thresholds are a floor**, never lowered to make CI pass.
- **`extract/` touches no filesystem and reads no clock.** Both would make
  the analysis untestable in the same way. `scan.rs` reads the clock and
  hands down a `Clock`.
- **No inline `#[allow(...)]` anywhere** — a CI job greps for it. Fix the
  lint or add a commented relaxation to `[lints.clippy]` in `Cargo.toml`.
- **Every claim must be provable.** No number, format or behaviour goes into
  the README, SPEC.md or the help text unless the code backs it, and the
  measurements that appear in a test's module doc name the machine they were
  taken on.
- **A change to the report is a change to the corpus.** `fixtures/` pins
  whole documents and whole answers; update them in the same commit, with a
  CHANGELOG entry describing the behaviour change.
- **Commits are conventional** (`feat:`, `fix:`, `docs:`, `test:`, `ci:`…),
  imperative. A hook enforces the shape.
