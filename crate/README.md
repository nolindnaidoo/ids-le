# ids-le

Find every identifier in a codebase, decode the time inside it, and
refuse the ones that cannot be named.

UUID (all versions), ULID, NanoID, MongoDB ObjectId and Snowflake. For
each one: what it is, where it is — line, column, and the document's own
key path — whether it is valid, and, where the identifier embeds a
timestamp, that timestamp as an ISO-8601 UTC string.

That last part is the point. Six of these carry a clock — UUID v1, v6 and
v7, ULID, ObjectId, Snowflake — in six unrelated bit layouts, over four
different epochs: the Gregorian reform of 1582, the Unix epoch, and the
two a Snowflake might have been minted against. Reading them uniformly is
work nobody wants to do twice.

```console
$ ids-le config.json
config.json:3:12  uuid v4  f47ac10b-58cc-4372-a567-0e02b2c3d479
config.json:4:19  uuid v7  019ff344-cc00-7abc-8def-0123456789ab  2026-08-12T00:00:00.000Z
config.json:6:25  ulid  01KZSM9K00ABCDEFGH12345678  2026-08-12T00:00:00.000Z
config.json:7:27  objectid  6a7bb780a1b2c3d4e5f60718  2026-08-12T00:00:00.000Z
config.json:8:19  refused (nil_or_max)  00000000-0000-0000-0000-000000000000  — the nil UUID: 128 zero bits, which RFC 9562 defines as naming nothing
4 identifiers in 1 file
1 run refused
```

(That is stderr. stdout carried the same five rows as one JSON line.)

## It refuses rather than guesses

A tool that answers confidently and wrongly is worse than one that stops
and names what it needs. So every run this crate will not name is **a row
in the report with a reason**, never a dropped row and never a silent
guess:

| Reason | What it means |
|---|---|
| `ambiguous_kind` | Two or more schemes fit and nothing in the document chooses between them. |
| `malformed` | The right shape, and validation failed. |
| `nil_or_max` | The nil or max UUID: structurally perfect, and RFC 9562 says it names nothing. |
| `version_claim_mismatch` | A UUID claims v4 — 122 random bits — and the bytes are plainly not random. Both are reported; neither is resolved. |
| `timestamp_implausible` | A decode landed before 1990 or more than a year out. The decode comes with the flag. |

Some of these are worth spelling out, because they are the cases a
regex-based tool gets confidently wrong:

- `5d41402abc4b2a76b9719d911017c592` is an unhyphenated UUID **and** an
  MD5 digest. Nothing in a document separates them, so nothing here picks.
- `6a7bb780a1b2c3d4e5f60718` is 24 hex characters. Under `_id` it is an
  ObjectId minted on 2026-08-12; under `checksum`, or in prose, it is
  refused. An ObjectId's whole specification is *24 hex characters*, so a
  truncated SHA-1 fits it exactly and only the document can separate
  them.
- `1536886938009600000` under `channel_id` is a Discord Snowflake at
  2026-08-12. Under a bare `user_id` it is refused, because the Twitter
  epoch fits too and the document does not say which. Under `population`
  it is not a finding at all.

The full table, and the boundaries the tool holds, are in
[SPEC.md](SPEC.md).

## Install

```console
cargo install ids-le          # once published
cargo install --path crate    # from a checkout, today
```

## Use it

```console
ids-le src/                    # a tree
ids-le --kind uuid src/        # one scheme
ids-le --strict src/           # fail on anything that could not be named
ids-le --hidden src/           # include .env
cat config.yaml | ids-le --stdin --format yaml
```

stdout is protocol — one JSON object per line, one line per file. stderr
is for you. There is no `--json` flag: one mode, and the human summary is
a projection of the same reports so the two cannot drift.

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
    }
  ],
  "diagnostics": [],
  "summary": { "ids": 1, "refused": 0 }
}
```

Exit codes follow grep, so a shell can branch on them:

| Code | Meaning |
|---|---|
| `0` | Identifiers found |
| `1` | None found — an answer, not an error |
| `2` | Malformed question |

A refusal does not move the exit code. Refusing is the tool working;
`--strict` is how a pipeline turns it into a failure.

## Formats

JSON (and JSONC), YAML, TOML, INI (`.cfg`, `.conf`, `.properties`),
dotenv and CSV get each finding a key path — `service.requestId`,
`documents.[0]._id`, `discord.channel_id`. Everything else is read as
text: **the same runs, in the same places, without the key**. That is why
you can point this at a repository nobody has described to it and get an
answer from the `.md`, the `.sql` and the `.tf` as well as the config.

The key path is evidence, not decoration. ObjectId and Snowflake are
named only under a field the document calls an id, so a run that is named
in the `.json` comes back `ambiguous_kind` in the `.md` beside it — same
row, same position, same decode, and a reason instead of a name.

## For agents

```console
ids-le mcp
```

Speaks the Model Context Protocol on stdio and offers two tools:
`extract_ids`, which reads a document handed to it and touches no
filesystem, and `ids_le_scan`, which reads files and directories. Both
return one envelope — `{ ok, data, diagnostics, meta }` — where `ok`
means the check ran, never that the answer was yes.

Refusals reach the agent as rows, exactly as they reach the terminal. An
agent that received only the identifiers this crate was willing to name
would conclude a document was clean when what actually happened is that
nothing in it could be named.

## What it will not do

It does not generate identifiers, rewrite them, redact them, or decide
whether one should be where it is. It reads; nothing is written. It never
touches the network. Full list in [SPEC.md](SPEC.md), "Non-goals".

## Part of LE Tools

One of a family of small, single-purpose extractors — `numbers-le`,
`urls-le`, `paths-le`, `secrets-le`, and the rest — at
[letools.dev](https://letools.dev). Each stands on its own: no shared
crate, no published core.

## License

MIT
