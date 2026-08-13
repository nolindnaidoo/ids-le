# Instructions for AI coding assistants

Read [AGENTS.md](AGENTS.md) first — it is the engineering-standards
document for this crate and the source of truth for layout, control-flow
style, the settled decisions, testing requirements and the definition of
done. [SPEC.md](SPEC.md) defines the product behavior. AGENTS.md wins on
any conflict.

- Before declaring any change complete, run exactly what CI runs:
  `cargo fmt --all --check`,
  `cargo clippy --all-targets -- -D warnings`,
  `cargo test --locked`. All three must pass.
- Never add an inline lint attribute — not `#[allow]`, not `#[expect]`.
  Fix the lint, or add a commented relaxation to `[lints.clippy]` in
  `Cargo.toml`. Four are there already, each with its reason.
- New logic goes in `extract/` when it is pure — it must then be unit
  tested, and it carries a **75% line coverage floor per module**,
  enforced per module rather than on the total so one cannot slide while
  the others carry it.
- **Refuse rather than guess.** A run that fits two schemes is refused
  with **both named**, and the refusal carries the evidence behind it,
  including the decode that caused it. Never a dropped row, never a
  silent success. A test that passes by resolving something that should
  have been refused is the bug this family exists to prevent.
- **Two leniencies are sanctioned and there is not a third**: an
  unrecognised *format* falls back to a text read, and a *document* a
  reader cannot parse costs key paths and never findings. Neither
  extends to the tool's own inputs — a bad `--kind`, a non-positive
  `maxResults`, an unopenable path are all refusals with a reason.
- **A key path is evidence for four kinds**, which is why every reader in
  `extract/*.rs` states its own limits in its own module doc. A reader
  that mislabels a value turns a refusal into a finding.
- **No reachable panic.** No `unwrap`, no panicking index, no arithmetic
  a document can overflow — `overflow-checks` stays on in release, so
  that last one crashes rather than quietly writing a wrong number into a
  report whose whole value is honesty. `expect` is permitted in exactly
  one shape: the invariant is established by a check **in the same
  function** and the message names it. AGENTS.md tables every existing
  site; a new one needs the same argument. `extract::position` is the
  counter-example worth copying — it clamps and floors instead.
- **Refusals speak the caller's vocabulary.** An MCP caller has no
  command line, so no message on that surface names a flag; a test greps
  for `--` across every tool definition and failure path.
- **No network, ever, and nothing is written** outside tests. No future
  feature verifies an identifier against a database or an API.
- The gated suites are opt-in and CI sets them: `IDS_LE_BUDGET=1`,
  `IDS_LE_FUZZ_SECONDS`, `IDS_LE_FUZZ_SEED`, `IDS_LE_SCENARIOS=1`. A
  skipped case says so by name; a skip is never reported as a pass.
- Write a regression test for every bug you fix, and **observe it fail
  before the fix** rather than assuming it would have. Run the binary
  against a real tree, not only the suite — that is where this family's
  invisible defects have come from.
