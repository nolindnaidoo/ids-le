<p align="center">
  <img src="https://raw.githubusercontent.com/nolindnaidoo/ids-le/main/assets/icon.png" alt="IDs-LE logo" width="96" height="96"/>
</p>
<h1 align="center">IDs-LE</h1>
<p align="center">
  <b>Find every identifier in a codebase, decode the time inside it, and refuse the ones that cannot be named</b><br/>
  <i>UUID · ULID · NanoID · MongoDB ObjectId · Snowflake</i>
</p>

<p align="center">
  <a href="https://crates.io/crates/ids-le">
    <img src="https://img.shields.io/crates/v/ids-le?style=for-the-badge&label=Rust%20CLI&color=blue&logo=rust" alt="ids-le on crates.io" />
  </a>
  <a href="https://crates.io/crates/ids-le">
    <img src="https://img.shields.io/crates/d/ids-le?style=for-the-badge&label=Downloads&color=blue" alt="crates.io downloads" />
  </a>
  <a href="https://github.com/nolindnaidoo/ids-le/actions/workflows/ci-crate.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/nolindnaidoo/ids-le/ci-crate.yml?branch=main&style=for-the-badge&label=CI&color=blue&logo=githubactions&logoColor=white" alt="CI" />
  </a>
  <a href="https://github.com/nolindnaidoo/ids-le/blob/main/crate/Cargo.toml">
    <img src="https://img.shields.io/badge/rustc-1.88+-blue?style=for-the-badge&logo=rust" alt="MSRV: Rust 1.88+" />
  </a>
  <a href="https://github.com/nolindnaidoo/ids-le/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-blue?style=for-the-badge" alt="MIT licensed" />
  </a>
  <a href="https://letools.dev/tools/ids-le">
    <img src="https://img.shields.io/badge/LE%20Tools-letools.dev-blue?style=for-the-badge" alt="LE Tools" />
  </a>
</p>

---

<p align="center">
  <img src="https://raw.githubusercontent.com/nolindnaidoo/ids-le/main/assets/demo.gif" alt="IDs-LE demo — the real binary, recorded by assets/demo.tape" style="max-width: 100%; height: auto;" />
</p>

> **Useful?** A star is how other developers find it —
> [★ GitHub](https://github.com/nolindnaidoo/ids-le) ·
> [letools.dev/tools/ids-le](https://letools.dev/tools/ids-le)

## What it does

Point it at a file or a tree. For every identifier it finds it reports the
kind, the raw text, where it is — line, column, and the document's own key
path — whether it is valid, and, where the identifier embeds a timestamp,
that timestamp as an ISO-8601 UTC string.

```console
$ ids-le ids.json
ids.json:3:12  uuid v4  f47ac10b-58cc-4372-a567-0e02b2c3d479
ids.json:4:19  uuid v7  019ff344-cc00-7abc-8def-0123456789ab  2026-08-12T00:00:00.000Z
ids.json:7:14  ulid  01KZSM9K00ABCDEFGH12345678  2026-08-12T00:00:00.000Z
ids.json:8:16  nanoid  V1StGXR8_Z5jdHi6B-myT
ids.json:11:15  objectid  6a7bb780a1b2c3d4e5f60718  2026-08-12T00:00:00.000Z
ids.json:13:30  snowflake  1536886938009600000  2026-08-12T00:00:00.000Z
ids.json:14:19  refused (nil_or_max)  00000000-0000-0000-0000-000000000000  — the nil UUID: 128 zero bits, which RFC 9562 defines as naming nothing
6 identifiers in 1 file
1 run refused
```

That is stderr. stdout carried the same seven rows as one JSON line, which is
what a pipeline reads:

```json
{
  "schema": 1,
  "file": "ids.json",
  "format": "json",
  "ids": [
    {
      "kind": "uuid",
      "value": "019ff344-cc00-7abc-8def-0123456789ab",
      "line": 4,
      "column": 19,
      "key": "service.requestId",
      "valid": true,
      "version": 7,
      "variant": "rfc4122",
      "timestamp": "2026-08-12T00:00:00.000Z"
    },
    {
      "kind": "uuid",
      "value": "00000000-0000-0000-0000-000000000000",
      "line": 14,
      "column": 19,
      "key": "placeholder",
      "valid": false,
      "refused": "nil_or_max",
      "detail": "the nil UUID: 128 zero bits, which RFC 9562 defines as naming nothing"
    }
  ],
  "diagnostics": [],
  "summary": { "ids": 6, "refused": 1 }
}
```

`kind` is always present, and `null` where naming a kind is exactly what was
refused — a reader has to be able to tell "no kind" from "field missing".
Columns are counted in **UTF-16 code units**, so they match what an editor
shows you.

## It refuses rather than guesses

A tool that answers confidently and wrongly is worse than one that stops and
names what it needs. So every run this crate will not name is **a row in the
report with a reason** — never a dropped row, never a silent guess.

| Reason | What it means |
|---|---|
| `ambiguous_kind` | Two or more schemes fit and nothing in the document chooses between them. |
| `malformed` | The right shape, and validation failed. |
| `nil_or_max` | The nil or max UUID: structurally perfect, and RFC 9562 says it names nothing. |
| `version_claim_mismatch` | A UUID claims v4 — 122 random bits — and the bytes are plainly not random. Both are reported; neither is resolved. |
| `timestamp_implausible` | A decode landed before 1990 or more than a year out. The decode comes with the flag. |

Some are worth spelling out, because they are the cases a regex gets
confidently wrong:

- `5d41402abc4b2a76b9719d911017c592` is an unhyphenated UUID **and** an MD5
  digest. Nothing in a document separates them, so nothing here picks.
- `6a7bb780a1b2c3d4e5f60718` is 24 hex characters. Under `_id` it is an
  ObjectId minted on 2026-08-12. Under `checksum`, or in prose, it is
  refused — an ObjectId's whole specification is *24 hex characters*, so
  a truncated SHA-1 fits it exactly and only the document can tell them
  apart.
- `1536886938009600000` under `channel_id` is a Discord Snowflake at
  2026-08-12. Under a bare `user_id` it is refused, because the Twitter epoch
  fits too and the document does not say which. Under `population` it is not
  a finding at all.

The full table, and the boundaries the tool holds itself to, are in
[`crate/SPEC.md`](crate/SPEC.md).

## Install

```console
cargo install ids-le
```

Or from a checkout:

```console
git clone https://github.com/nolindnaidoo/ids-le
cargo install --path ids-le/crate
```

Needs **Rust 1.88+**, and nothing else. No runtime, no network, nothing
written.

## Use it

```console
ids-le src/                    # a tree
ids-le --kind uuid src/        # one scheme
ids-le --strict src/           # fail on anything that could not be named
ids-le --hidden src/           # include .env
cat config.yaml | ids-le --stdin --format yaml
```

stdout is protocol — one JSON object per line, one line per file. stderr is
for you. There is no `--json` flag: one mode, and the human summary is a
projection of the same reports so the two cannot drift.

| Flag | Effect |
|---|---|
| `--kind <kind>` | Report only `uuid`, `ulid`, `nanoid`, `objectid` or `snowflake`. A view over the report, applied after the analysis — the unfiltered run is the complete one. |
| `--format <format>` | Force a format. An unknown name reads the text directly rather than failing. |
| `--strict` | Exit 2 on any refusal or unreadable text file. |
| `--stdin` | Read one document from stdin. Takes no file arguments. |
| `--hidden` | Walk hidden files and directories — which is where `.env` lives. |
| `--no-ignore` | Walk files `.gitignore` excludes. |

## The kinds

| Kind | Shape | Decoded time | Extra fields |
|---|---|---|---|
| `uuid` | 36 characters, `8-4-4-4-12`, hex | v1, v6, v7 | `version`, `variant` |
| `ulid` | 26 characters, Crockford base32 | always (48-bit Unix ms) | — |
| `nanoid` | 21 characters, `A-Za-z0-9_-` | never — a NanoID has no clock | — |
| `objectid` | 24 hex characters, under a key naming an id | always (32-bit Unix seconds) | — |
| `snowflake` | 17–19 digits, under a key naming an id | always (top 42 bits + an epoch) | — |

**Two of those kinds need the document's permission.** An ObjectId is 24
hex characters and a Snowflake is a large integer; neither carries a
version, a variant, a checksum or a restricted alphabet, so neither run
can say on its own what it is — a truncated git hash is a structurally
perfect ObjectId, and a byte count is a structurally perfect Snowflake.
Both are named only where the field's own name ends in `id` (`_id`,
`userId`, `USER-ID`, `$oid`), and refused as `ambiguous_kind` otherwise.
In a plain-text file, which has no keys at all, neither is ever named.

All eight UUID versions RFC 9562 defines are recognised, and all four
variants — `ncs`, `rfc4122`, `microsoft`, `future` — are reported. A version
is only reported under the RFC variant, because in any other layout those
four bits are not a version field.

## The timestamps

This is the part nobody wants to write twice. Six of these carry a clock, in
six unrelated bit layouts, over three different epochs — one of which starts
in 1582:

| Scheme | Where the time is | Epoch |
|---|---|---|
| UUID v1 | 60 bits of 100-nanosecond intervals, low field first | 1582-10-15 |
| UUID v6 | the same ticks, reordered so they sort | 1582-10-15 |
| UUID v7 | 48 bits of milliseconds, at the front | Unix |
| ULID | the first ten Crockford characters, 48 bits | Unix |
| ObjectId | the leading four bytes, in seconds | Unix |
| Snowflake | the top 42 bits | Twitter (2010-11-04) or Discord (2015-01-01) |

Every one comes back as the same ISO-8601 UTC string, milliseconds always
present. UUID v2 is named and **not** decoded: its clock has its low 32 bits
overwritten by a POSIX uid, so it resolves to roughly seven minutes and is
not the same claim.

A decode that lands before 1990 or more than a year from now is refused as
`timestamp_implausible` — and the decode is on the row next to the flag,
because a refusal that hides its evidence is a verdict a reader cannot check.

## Exit codes

Follow grep, so a shell can branch on them:

| Code | Meaning |
|---|---|
| `0` | Identifiers found |
| `1` | None found — an answer, not an error |
| `2` | The question was malformed, or a scan gave up part way |

**A refusal does not move the exit code.** Refusing is the tool working;
`--strict` is how a pipeline turns it into a failure. A binary file — a NUL
byte in its first 8 KiB, ripgrep's own test — is never a text candidate: no
report line, counted on stderr, and it never fails the run.

## Formats

JSON (and JSONC), YAML, TOML, INI (`.cfg`, `.conf`, `.properties`), dotenv
and CSV give each finding a key path — `service.requestId`,
`documents.[0]._id`, `discord.channel_id`. Everything else is read as text:
**the same runs, in the same places, without the key**. That is why you can
point this at a repository nobody has described to it and get an answer out
of the `.md`, the `.sql` and the `.tf` as well as the config.

The key path is evidence, not decoration. ObjectId and Snowflake are named
only under a field the document calls an id, so a run that is named in the
`.json` comes back `ambiguous_kind` in the `.md` beside it — same row, same
position, same decode, and a reason instead of a name.

## For agents

```console
ids-le mcp
```

Speaks the Model Context Protocol on stdio and offers two tools:
`extract_ids`, which reads a document handed to it and touches no filesystem,
and `ids_le_scan`, which reads files and directories. Both return one
envelope — `{ ok, data, diagnostics, meta }` — where `ok` means the check
ran, never that the answer was yes.

Refusals reach the agent as rows, exactly as they reach the terminal. An
agent that received only the identifiers this crate was willing to name would
conclude a document was clean when what actually happened is that nothing in
it could be named.

## What it will not do

It does not generate identifiers, rewrite them, redact them, or decide
whether one should be where it is. It reads; nothing is written. It never
touches the network, and it verifies nothing against a database or an API.
Full list in [`crate/SPEC.md`](crate/SPEC.md), "Non-goals".

## Development

The crate is in [`crate/`](crate/). Gates, in the order CI runs them:

```bash
cd crate
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

Beyond those, five suites that do not run on a laptop mid-edit:

| Suite | What it holds |
|---|---|
| `tests/hazards.rs` | A BOM, invalid UTF-8, UTF-16, a FIFO, a mode-000 file, a symlink loop, a 300-character path, an empty file, a multi-megabyte minified line, a 3 MB base64 blob — on a tree built at runtime, on all three platforms |
| `tests/platform.rs` | Forward slashes in every reported path, case folding, reserved Windows names, CRLF, stdin — and the whole suite under three time zones |
| `tests/fuzz.rs` | Generated runs at every boundary the classifier decides on; `IDS_LE_FUZZ_SECONDS` sets the clock |
| `tests/budget.rs` | A wall-clock ceiling and three linearity checks; `IDS_LE_BUDGET` runs it |
| `tests/coverage_matrix.rs` | Every kind, reason, UUID version, variant and format reachable from a real fixture |

Architecture and conventions live in [`crate/AGENTS.md`](crate/AGENTS.md);
behaviour in [`crate/SPEC.md`](crate/SPEC.md); changes in
[`CHANGELOG.md`](CHANGELOG.md).

## More from the LE family

Sixteen single-purpose tools for the work in front of every model. Each ships
a Rust CLI and an MCP server. One page: **[letools.dev](https://letools.dev)**

**Get it out**

- **[String-LE](https://letools.dev/tools/string-le)** — Extract every string in a codebase, with its position, so a person can read them
- **[Numbers-LE](https://letools.dev/tools/numbers-le)** — Extract every hardcoded number in a codebase, so a person can check them
- **[Units-LE](https://letools.dev/tools/units-le)** — Extract every quantity with its unit, normalized, and refuse the ambiguous ones by name
- **[Dates-LE](https://letools.dev/tools/dates-le)** — Extract every date and timestamp, and the exact instant each one resolves to
- **[IDs-LE](https://letools.dev/tools/ids-le)** — Extract every UUID, ULID, NanoID, ObjectId and Snowflake, and decode the time inside
- **[IPs-LE](https://letools.dev/tools/ips-le)** — Extract every IP address, CIDR block and MAC, normalized and classified by scope
- **[URLs-LE](https://letools.dev/tools/urls-le)** — Extract every URL in a codebase, with its protocol and exact position
- **[Paths-LE](https://letools.dev/tools/paths-le)** — Extract every file path in a codebase, and say whether it still points at anything
- **[Colors-LE](https://letools.dev/tools/colors-le)** — Extract every color in a codebase, and say which ones are not in your palette

**Check it**

- **[Regex-LE](https://letools.dev/tools/regex-le)** — Find every regex in a codebase, and report which can be driven into catastrophic backtracking
- **[Versions-LE](https://letools.dev/tools/versions-le)** — Find where one dependency is constrained differently across a repository's manifests
- **[i18n-LE](https://letools.dev/tools/i18n-le)** — Identify the i18n library a project uses, then audit its catalogs by that library's rules
- **[Scrape-LE](https://letools.dev/tools/scrape-le)** — Check whether a page is scrapeable before the scraper is written, and say when it cannot tell

**Guard it**

- **[Secrets-LE](https://letools.dev/tools/secrets-le)** — Find hardcoded credentials in a codebase, and never print one into the report
- **[EnvSync-LE](https://letools.dev/tools/envsync-le)** — Compare the dotenv files in a tree, and say which keys are missing from which
- **[Unicode-LE](https://letools.dev/tools/unicode-le)** — Find the Unicode that hides meaning — bidi controls, invisibles, homoglyphs, mixed scripts

Each stands on its own: no shared crate, no published core. Where two of them
agree, it is because the same answer was right twice.

**Contact** — [nolindnaidoo.com](https://nolindnaidoo.com) · [GitHub](https://github.com/nolindnaidoo) · [LinkedIn](https://www.linkedin.com/in/nolindnaidoo/)
## Also by nolindnaidoo

**Rust** — pixelcoords and pixelactions are one loop: pixelcoords answers
*where*, pixelactions *acts* there. Their own tools, their own voice — not
part of the LE family.

- **[pixelcoords](https://github.com/nolindnaidoo/pixelcoords)** — Freeze your screen, mark regions, get pixel-exact coordinates and crops
  [pixelcoords.dev](https://pixelcoords.dev) · [crates.io](https://crates.io/crates/pixelcoords) · [docs.rs](https://docs.rs/pixelcoords)
- **[pixelactions](https://github.com/nolindnaidoo/pixelactions)** — Consume human-verified coordinates, perform the interaction, confirm it landed
  [pixelactions.dev](https://pixelactions.dev) · [crates.io](https://crates.io/crates/pixelactions) · [docs.rs](https://docs.rs/pixelactions)

## License

MIT © [nolindnaidoo](https://github.com/nolindnaidoo)
