# CLAUDE.md

[AGENTS.md](AGENTS.md) is the technical source of truth for this repo, and it
routes you to [crate/AGENTS.md](crate/AGENTS.md) — the engineering standard
the code is actually held to: control flow, error handling, structure, the
settled decisions, the definition of done. Read it before writing code.
[crate/SPEC.md](crate/SPEC.md) defines the product behaviour. README.md is
user-facing.

**This repo is crate-only.** There is no VS Code extension beside the crate
yet, so there is no parity corpus and no second implementation to be held
equal to — `crate/fixtures/` is a characterisation record of this crate's own
behaviour, and its job is that a change to the report is deliberate and
visible in a diff.

## Where to look

| Question | File |
|---|---|
| How should this code be written? | [crate/AGENTS.md](crate/AGENTS.md) — the standard, plus the architecture and the invariants |
| What does the tool do? | [crate/SPEC.md](crate/SPEC.md) — kinds, refusals, exit codes, both surfaces |
| What does the user see? | [README.md](README.md) |
| What changed? | [CHANGELOG.md](CHANGELOG.md) and [crate/CHANGELOG.md](crate/CHANGELOG.md) |

## Gates

```bash
cd crate
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --locked
```

Before a release, also the suites CI gates off a laptop:

```bash
IDS_LE_SCENARIOS=1 cargo test --release --test scenarios
IDS_LE_FUZZ_SECONDS=60 cargo test --release --test fuzz -- --nocapture
IDS_LE_BUDGET=1 cargo test --release --test budget -- --nocapture --test-threads=1
cargo test --test hazards -- --nocapture
cargo test --test platform -- --nocapture
cargo test --test coverage_matrix -- --nocapture
```

## Things that will bite you

- **Refusing is the product.** Everything else here could be rebuilt from a
  regex and a bit shift. A run that fits two schemes is refused with both
  named; a refusal carries its reason *and* the decode that caused it. Never
  a dropped row, never a silent success.
- **`extract/` touches no filesystem and reads no clock.** A plausibility
  window that moved with the wall clock would fail every test on a future
  Tuesday. `scan.rs` reads the clock and hands down a `Clock`; the corpus
  runs against a pinned one.
- **A skipped case is never a pass.** `hazards.rs` and `platform.rs` print
  `SKIPPED <case>: <why>` on stderr where a platform cannot express a case,
  and the CI jobs run with `--nocapture` so the log says what was not
  checked.
- **The `coverage-matrix` marker line is load-bearing.** `cargo test
  <filter>` exits 0 when nothing matches, so the CI job greps stdout for
  `coverage-matrix: complete`. Do not change the string without changing the
  job.
- **Timing numbers in a test's module doc name the machine they came from.**
  If a ceiling is tight on a runner, re-measure there and say so in the note
  — never quietly raise the number.
- **CI narrows itself on a docs-only push.** `ci-crate.yml` fires on `*.md` and
  the agent instruction files — it has to, because the `policy` job greps them,
  and the filter used to admit only `crate/**` so that gate could run only when
  the files it guards had *not* been touched. On a docs-only push `policy` and
  `commits` run and every Rust job skips. Anything unrecognised, and an
  unreadable diff, counts as code and runs everything.
- **Coverage floors are a backstop, not a target** — well below where the code
  actually is, and never raised to track it. 75% per
  module in `extract/`.
- **No inline `#[allow(...)]`** — a CI job greps for it, and a test is not an
  exemption. Fix the lint or relax it visibly in `[lints.clippy]`.
- **Every claim must be provable.** A number in the README, SPEC.md or a
  module doc has to be backed by code or by a measurement that names its
  conditions. That governs **behaviour and numbers**, not **availability**: an
  install line for a publish you are about to make is **staged, not
  forbidden**. Write it, and let the release commit be what makes it true.
