# CLAUDE.md

[AGENTS.md](AGENTS.md) is the technical source of truth for this repo: the
engineering standard the code is held to — control flow, error handling,
immutability, structure — plus this repo's architecture, invariants, toolchain
and release. Read it before writing code. README.md is user-facing and partly
generated.

The repo also hosts the Rust CLI in `crate/` — read `crate/CLAUDE.md` and
`crate/AGENTS.md` for that side; the shared corpus is `crate/fixtures/`.

## Where to look

| Question | File |
|---|---|
| How should this code be written? | [AGENTS.md](AGENTS.md) — the standard, plus this repo's architecture and invariants |
| What does the user see? | [README.md](README.md) — Testing and Performance are generated |
| What changed? | [CHANGELOG.md](CHANGELOG.md) |

## Gates

```bash
bun run typecheck && bun run lint && bun run test
```

Before a release, also `bun run test:integration`, `bun run package`, and
`bun run test:e2e-vsix` — the last is the only test that exercises the
artifact users actually install.

## Things that will bite you

- **Two README sections are generated.** Testing and Performance sit between
  `<!-- coverage:start -->` / `<!-- performance:start -->` markers and come
  from `scripts/coverage-readme.js` and `scripts/perf-readme.js`. Edit the
  code and regenerate; do not type numbers in by hand. They are regenerated
  at release, and CI does not check them.
- **A refusal is a row, never a dropped one.** Refusing is the product: a run
  that fits two schemes is reported with both named, and a refusal carries its
  reason *and* the decode that caused it. The report, the notification and the
  MCP answer all count refusals. A filter that hid one would make a document
  that could not be named look clean.
- **Every claim must be provable.** No feature, metric or format goes in a
  README, the manifest, or help text unless the code backs it. That governs
  **behaviour and numbers** — not **availability**. Whether something is
  published, listed or installable is a fact about a registry at a moment in
  time, and it is false right up until you make it true. Copy for a release you
  are about to make is **staged, never forbidden**: write it, and let the
  release commit be what makes it true.
- **This repo is one of the family's extension repos.** The shared config
  files, scripts and workflows are byte-identical across them, and
  `letools-site/scripts/check-fleet.ts` is what holds them there rather than
  memory: run `bun run check:fleet ../` from a checkout of the site with the
  others beside it, or dispatch its **Fleet** workflow. It names the file and the
  repos that drifted, so a missed copy is a report rather than something you
  find months later. Anything under `crate/` is outside the check on purpose —
  the crates stand on their own.
- **The extraction is shared with the Rust CLI**, and the corpus under
  `crate/` is the contract. Changing extraction behaviour means changing
  `crate/src/extract/` and `src/extract/` together, updating the corpus, and
  running `bun scripts/check-extraction-parity.ts` and the differential. CI
  fails when either side drifts.
- **What the contract holds equal is the shared `extract_ids` MCP tool**, which
  both servers offer and must answer identically; a difference there is a bug.
  **The surfaces are meant to differ.** This one is IDE-first — the active
  document, and a report a person reads. The CLI is terminal-first: a tree
  walk, exit codes, `--strict` and one JSON line per file, none of which has an
  editor equivalent. That is not drift, and nothing holds them equal — see
  `crate/SPEC.md`.
- **`src/extract/` reads no clock.** The command and the MCP tool read now once
  and pass a `Clock` down, as the crate's `scan.rs` does; the corpus and the
  tests pin one. A plausibility window that moved with the wall clock would fail
  every test on a future Tuesday.
- **Offsets are UTF-16, the crate's are bytes.** Every byte the crate compares
  is ASCII, so the logic ports unchanged — except YAML indentation, which is
  compared in UTF-8 bytes across lines. Keep it that way; a test pins it.
- **Localization is two mechanisms, and they fail separately.** `src/i18n/package.nls.*.json`
  covers the manifest; `l10n/bundle.l10n.*.json` covers runtime strings through
  `vscode.l10n.t()`. Twelve locales each, held in exact key parity by the
  integration test. Never call `l10n.t()` at module scope, never compare a
  translated label against an English literal, and use positional `{0}`
  placeholders rather than template literals.
- **CI narrows itself on a docs-only push.** A change touching only `*.md` and
  `LICENSE` runs the Linux leg alone and skips the version gate; `ci-crate.yml`
  runs its `policy` gate with every Rust job skipped. Nothing that covers the
  change is skipped — the integration suite and the installed-VSIX end-to-end
  are Linux-only anyway. Anything unrecognised, and an
  unreadable diff, counts as code and runs everything. A release commit always
  touches `package.json`, so a release still sees the full three-OS matrix.
- **Coverage floors are a backstop, not a target.** They sit well below where
  the code actually is, and they are not raised to track it — a floor that
  follows real coverage becomes a tax on writing the next module.
