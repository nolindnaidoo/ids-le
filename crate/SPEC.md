# ids-le — behaviour specification

What the tool does, what it refuses to do, and what a caller may rely
on. [AGENTS.md](AGENTS.md) is how the code gets there and wins on any
conflict about *how*; this file wins about *what*.

Version 0.1.0. The report shape is versioned on every line (`schema: 1`)
and the exit codes are the API — both change only with a major version
and a CHANGELOG entry.

## What it is

Point it at a file or a tree. For every identifier it finds it reports
the kind, the raw text, the position (line, column, and the document's
own key path), whether it is valid, and — where the identifier embeds a
time — that time decoded to an ISO-8601 UTC string.

A year outside `0000`–`9999` is written in ISO-8601's **expanded**
representation: a sign and six digits, `+010889-08-02T05:31:50.655Z`, the
same shape `Date.toISOString` produces. Only a refusal ever carries one —
the plausibility window ends a year past now — and the largest instant any
kind here can decode is 10889, from a ULID or a UUID v7 whose 48-bit
millisecond field is all ones.

Where it cannot name a run honestly, it says so with a reason instead of
picking one.

## Kinds

| Kind | Shape | Decoded time | Extra fields |
|---|---|---|---|
| `uuid` | 36 characters, `8-4-4-4-12`, hex | v1, v6, v7 | `version`, `variant` |
| `ulid` | 26 characters, Crockford base32 | always (48-bit Unix ms) | — |
| `nanoid` | 21 characters, `A-Za-z0-9_-` | never — a NanoID has no clock | — |
| `objectid` | 24 hex characters, **under a key naming an id** | always (32-bit Unix seconds) | — |
| `snowflake` | 17–19 digits, **under a key naming an id** | always (top 42 bits + an epoch) | — |

`variant` is one of `ncs`, `rfc4122`, `microsoft`, `future`. `version` is
reported only under the RFC 4122 variant, because in any other layout
those four bits are not a version field.

UUID timestamps: v1 and v6 read 60 bits of 100-nanosecond intervals since
1582-10-15 (in two different orders); v7 reads 48 bits of Unix
milliseconds. v2 is named but not decoded — its clock has its low 32 bits
overwritten by a POSIX uid, so it resolves to roughly seven minutes and
is not the same claim.

Snowflake epochs: Twitter (`1288834974657`) and Discord
(`1420070400000`).

## Refusals

A refusal is **a row in the report**, never a dropped one. It carries the
raw text, the position, the key path, `valid: false`, a `refused` reason,
a `detail` sentence, and whatever was decoded before the refusal — the
version, the variant, the timestamp that made the reason true.

| Reason | When | Example |
|---|---|---|
| `ambiguous_kind` | Two or more schemes fit and nothing in the document chooses. | `5d41402abc4b2a76b9719d911017c592` — 32 hex digits are an unhyphenated UUID and an MD5 digest in equal measure. |
| `ambiguous_kind` | A structurally valid ULID that is not canonically uppercase, under a key that does not say ULID. | `01kzsm9k00AbCdEfGh12345678` |
| `ambiguous_kind` | 21 base62 characters — a NanoID, a short token and a truncated hash all fit — with no `-`/`_` and no key naming the scheme. | `V1StGXR8xZ5jdHi6BxmyT` |
| `ambiguous_kind` | 24 hex digits under a key that does not name an identifier — or none at all, as in every plain-text file. An ObjectId has no structure to check, so context is the only evidence there is. | `6a7bb780a1b2c3d4e5f60718` under `checksum` |
| `ambiguous_kind` | 24 hex digits under a key that *does* name an identifier, whose leading four bytes are not a plausible time. Both signals are required. | `e83c5163316f89bfbde7d9ab` under `commitId` — a truncated git hash. |
| `ambiguous_kind` | A Snowflake under a key that names no platform, where both epochs decode to a plausible instant. | `1536886938009600000` under `user_id` |
| `malformed` | The right shape, and validation failed: a non-RFC UUID variant, a version outside 1–8, a non-hex character in the `8-4-4-4-12` shape, a ULID character outside Crockford base32, a ULID leading character above `7`, or a hex run one digit either side of 24 or 32. | `f47ac10b-58cc-4372-1567-0e02b2c3d479` (NCS variant) |
| `nil_or_max` | The nil UUID or the max UUID. Structurally valid; RFC 9562 defines both as naming nothing. | `00000000-0000-0000-0000-000000000000` |
| `version_claim_mismatch` | A UUID claiming v4 — 122 random bits — with twelve or more zero nibbles in a row outside the version and variant positions. The claim and the doubt are both reported; neither is resolved. | `00000000-0000-4000-8000-000000000000` |
| `timestamp_implausible` | A decode landed before 1990-01-01 or more than a year after now. The decode is on the row next to the flag. | `7fffffff-ffff-7abc-8def-0123456789ab` — a v7 in the year 6429. |

The plausibility window is `[1990-01-01T00:00:00Z, now + 365 days]`. Now
is read once per run, on the surface, never inside the analysis.

### Where the boundary sits

These are the stated limits, not oversights.

- **A structureless run needs the document's word for it.** Two of the
  five kinds have nothing to validate, and both are held to the same
  rule: **the leaf of the key path must end in `id`** (compared with
  separators removed and case folded, so `_id`, `userId`, `USER-ID`,
  `$oid` and `objectId` are the same evidence). One predicate,
  `policy::names_an_id`, serves both — two definitions of "the document
  names this an identifier" would drift.

  - **A bare integer is not a Snowflake.** Nothing *in* an 18-digit
    integer says it is an identifier — it is equally a byte count, a
    microsecond timestamp, an account number. 17–19 digits **and** a key
    naming an id (or mentioning `snowflake`) are required together.
  - **A bare 24-hex run is not an ObjectId.** An ObjectId's entire
    specification is *24 hex characters*: no version, no variant, no
    checksum, no reserved bits. Every 24-hex run is therefore a
    structurally perfect ObjectId, and a truncated SHA-1 or MD5 digest
    is exactly as perfect. A key naming an id **and** a plausible
    embedded timestamp are required together.

  In a document with no keys — every plain-text file — neither kind is
  ever named. That is the trade this crate makes on purpose: precision
  over recall, with the refusal naming the run, the reason and the
  decode so a reader can disagree.
- **The leaf of the key path is the field's own name, and that is what
  every kind reads.** ULID, NanoID and Snowflake can also be named by a
  key that says the scheme outright (`session.ulid`, `nanoId`,
  `snowflake`), and that too is the leaf: `ulids.count` is a count that
  happens to sit in a table about identifiers, not a ULID.

  The one exception is **which platform** minted a Snowflake, which reads
  the whole path — `discord.id` is a Discord id. Which scheme a field
  holds is a fact about the field; which platform minted it is a fact
  about the structure the field sits in.
- **Only the canonical hyphenated form is a UUID.** The 32-character
  unhyphenated form is refused as `ambiguous_kind`, because it is
  character-for-character an MD5 digest.
- **A prefixed identifier is not an identifier.** A candidate is a
  *maximal* run over `[0-9A-Za-z_-]`, so `user_550e8400-…` is one
  45-character run that matches no shape and produces no row. Deciding
  where the prefix ended would be guessing which part of a string is the
  identifier.
- **What the ObjectId rule costs, and what it bought.** The timestamp
  alone was never enough evidence, and the arithmetic says why: the
  leading field is a 32-bit count of seconds spanning
  2³² = 4,294,967,296 values, while the plausibility window runs from
  1990-01-01 (631,152,000) to now + 365 days — 1,818,028,800 on
  2026-08-12. That is 1,186,876,800 seconds, **27.6% of the space**, so
  timestamp-only naming admitted better than one random 24-hex run in
  four. Measured on `fixtures/documents/hashes.txt`, 600 abbreviated
  SHA-1 and MD5 digests: **163 named, 27.2%**. On a repository of git
  object names or truncated digests that is a false-positive in every
  fourth hash.

  Requiring the key takes that to **0 of 600**. The cost is recall: a
  genuine ObjectId in prose, or under a field called `checksum`, is now
  refused. The residual is stated rather than hidden — under a key that
  *does* name an id the timestamp is the only remaining filter, so
  roughly 27% of digests stored in a field called `commitId` are still
  named, and that number rises about 0.73 points a year as the window's
  upper edge tracks the clock. `tests/coverage_matrix.rs` asserts the
  first number is zero and prints both on every run.
- **Roughly half of real NanoIDs arrive as refusals.** A NanoID is
  recognisable only by its default 21-character base64url alphabet, and
  `-`/`_` — the two characters that separate base64url from base62 —
  appear in about 49% of them. A key naming the scheme settles the rest.
- **A 21- or 26-character run that does not look random is not a
  candidate at all.** A NanoID needs a digit and mixed case; a ULID needs
  a digit and a letter. Without those gates every 26-letter English word
  would be refused as a malformed ULID and the report would be unusable.

## The report

stdout is protocol: one JSON object per line, one line per file. stderr
is for the human and is a projection of the same reports — never a
parallel source of truth.

```json
{
  "schema": 1,
  "file": "config.json",
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
      "kind": null,
      "value": "5d41402abc4b2a76b9719d911017c592",
      "line": 6,
      "column": 14,
      "key": "digest",
      "valid": false,
      "refused": "ambiguous_kind",
      "detail": "32 hex digits are an unhyphenated UUID and an MD5 digest in equal measure; nothing in this document chooses between them"
    }
  ],
  "diagnostics": [],
  "summary": { "ids": 1, "refused": 1 }
}
```

- `kind` is always present, and `null` where naming a kind is exactly
  what was refused. A reader must be able to tell "no kind" from "field
  missing".
- `version`, `variant`, `timestamp`, `refused` and `detail` are absent
  when they do not apply.
- `key` is absent when the format has no keys, or the run sits outside
  every value region.
- Columns are 1-based and counted in **UTF-16 code units**, so they match
  what an editor shows.
- `summary.ids` counts named rows; `summary.refused` counts the rest.
  `summary.ids + summary.refused == ids.length`.

## Formats

`json` (and JSONC), `yaml`, `toml`, `ini` (`.cfg`, `.conf`,
`.properties`), `env`, `csv`, and `text` for everything else.

**The format changes only the key path, never which runs are found.** The
identifier scan is one scanner over raw text for every document, so a
`.md` file yields the same rows as the `.json` beside it, in the same
places. An unrecognised format is therefore leniency that costs key
paths, not a refusal.

It can cost a *verdict*, though, and that is not a contradiction: the key
path is **evidence**. ObjectId and Snowflake are named only under a key
naming an id, so the same run that is named in the `.json` comes back
`ambiguous_kind` in the `.md`. The row, the position and the decode are
all still there — what is missing is the thing that would justify naming
it, and the refusal says so.

**A comment is not part of a value, and not evidence.** Every reader that
has comment characters stops the value region at the first one that sits
outside quotes with whitespace in front of it — so a run inside a
trailing comment carries no key path, and cannot borrow the verdict a
key would have bought it. Quoting protects the character
(`id = "a # b"`), and a comment character with no space in front of it
belongs to the value (`a: b#c` is one YAML scalar).

Each reader is a line scanner rather than a parser, and states its own
limits:

- **JSON** — a byte scanner with a container stack. Comments and trailing
  commas tolerated; a document that does not parse still yields the key
  paths it read.
- **TOML** — table headers and one key per line. A multi-line array or
  inline table carries its key on the first line only. `[[array]]` sets
  the prefix without an index.
- **YAML** — block mappings and block sequences. Flow style, anchors,
  aliases, multi-line scalars and multi-document files are not modelled.
- **INI** — `[section]` plus `key = value`, or `key: value` where no `=`
  is present. `;` and `#` both comment.
- **dotenv** — `KEY=value`, `export` stripped, `#` outside quotes ends
  the value.
- **CSV** — the first row is the header and names the columns; a column
  with no name is `[n]`.

## Exit codes

The API. A shell branches on these.

| Code | Meaning |
|---|---|
| `0` | At least one identifier was named. |
| `1` | None were. Finding nothing is an answer, not an error. |
| `2` | The question was malformed. |

**A refusal does not move the exit code.** Refusing is the tool working.
`--strict` is for the pipeline that wants every run named or nothing
shipped: under it, any refusal *or* any text file that could not be read
exits 2.

**Nothing about a file exits 2 on its own.** An unknown flag, an unknown
kind, and a path that cannot be opened at all are malformed questions; a
file the filesystem refused mid-tree is a fact about the tree, reported
with a warning and carried. There is no "gave up part way" state — a file
is read whole or the reason it was not is named.

A binary file — a NUL byte in its first 8 KiB, ripgrep's test — is never
a text candidate: no report line, counted on stderr, and it never fails
the run, including under `--strict`.

## Command line

```
ids-le [options] <file|dir>...
ids-le [options] --stdin [--format <format>]
ids-le mcp
ids-le --version | --help
```

| Flag | Effect |
|---|---|
| `--kind <kind>` | Report only `uuid`, `ulid`, `nanoid`, `objectid` or `snowflake`. **A view over the report, applied after the analysis** — a refusal that named no kind disappears under any `--kind`, so the unfiltered run is the complete one. An unknown kind is refused. |
| `--format <format>` | Force a format. An unknown name reads the text directly rather than failing. |
| `--strict` | Exit 2 on any refusal or unreadable text file. |
| `--stdin` | Read one document from stdin. Takes no file arguments. |
| `--hidden` | Walk hidden files and directories — which is where `.env` lives. |
| `--no-ignore` | Walk files `.gitignore` excludes. |

There is no `--json` flag. One mode, nothing to misremember, and the
human summary is a projection of the same reports so the two cannot
drift.

Directories are walked with ripgrep's `ignore`, so what this reads and
what ripgrep reads are the same answer. A file named explicitly is always
read, ignore rules included. Symlinks are never followed.

## MCP

`ids-le mcp` speaks the Model Context Protocol on stdio, protocol version
`2025-06-18`, and offers two tools.

**Both schemas are strict in the same way**: `additionalProperties` is
`false`, and each says what it needs — `extract_ids` requires `content`,
`ids_le_scan` requires one of `path` or `paths`. A property neither
defines is a typo the caller should be told about, not a knob silently
ignored.

- **`extract_ids`** — takes `content` plus optional `format`, `filename`,
  `kind`, `maxResults`. Touches no filesystem.
- **`ids_le_scan`** — takes `path` or `paths`, plus optional `format`,
  `kind`, `hidden`, `ignored`. Reads the filesystem; never writes.

Every tool returns one envelope:

```json
{ "ok": true, "data": {}, "diagnostics": [], "meta": { "tool": "", "count": 0, "truncated": false } }
```

`ok` reports whether the check **ran**, not whether the answer is yes: a
document in which nothing could be named is `ok: true` with refusals in
it and a `refused` warning in `diagnostics`. A malformed question is a
JSON-RPC error; a tool that fails on its arguments returns a result
carrying `isError` so a model reads the reason and corrects itself.

Refusals speak the caller's vocabulary — no message on this surface names
a command-line flag.

## Non-goals

- **It does not generate identifiers.** Not a UUID, not a ULID, not one
  for testing. There is no flag for it and there will not be.
- **It does not rewrite, redact or repair anything.** It reads; the
  filesystem is never written.
- **It does not guess.** Where two schemes fit, both are named in the
  refusal and neither is chosen. Where evidence is missing, the row says
  what is missing.
- **It does not decide whether an identifier should be there.** A leaked
  key, a hardcoded id, a test fixture in production code — all are
  findings, none are verdicts. `secrets-le` is the tool with an opinion.
- **It does not verify an identifier against anything external.** No
  database, no API, no network of any kind, ever.
- **It does not accept an identifier scheme it cannot check.** Adding a
  kind means adding its structure, its refusals and its corpus cases
  together.
- **It does not resolve `version_claim_mismatch`.** Reporting both the
  claim and the doubt is the answer; deciding which is the mistake would
  need context this tool does not have.
