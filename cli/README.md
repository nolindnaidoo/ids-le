# ids-le

Find every identifier in a codebase: UUID at every version, ULID, NanoID,
MongoDB ObjectId and Snowflake. Each one comes back with where it is, what
it is, and the time inside it when it carries one.

```bash
npx ids-le .
npx ids-le --kind uuid src/ > ids.jsonl
git diff | npx ids-le --stdin
```

The command it installs is `ids-le`, the same command line as the Rust
crate [`ids-le`](https://crates.io/crates/ids-le), and it behaves
identically: same flags, same walk, same answers, same output, same exit
codes. Install whichever toolchain you already have.

## What it reports

JSON, YAML, CSV, TSV, TOML, INI and dotenv give each identifier the
document's own key path, such as `service.id`. Anything else is read as text
and reported without one.

**It refuses rather than guesses.** A run that fits two schemes, a UUID
whose variant bits are wrong, the nil UUID, a v4 that is plainly not random,
a timestamp before 1990: each is a row in the report with a named reason,
never a dropped row and never a silent answer.

Directories are walked the way ripgrep walks them: `.gitignore`, `.ignore`
and hidden files are respected unless you pass `--no-ignore` or `--hidden`.
A binary file, a NUL byte in its first 8 KiB, is counted and skipped.

## Output

stdout carries one JSON report per file whenever it is piped or redirected,
and nothing when it is a terminal. stderr lists every row.

```text
$ ids-le config.json
config.json:3:12  uuid v4  f47ac10b-58cc-4372-a567-0e02b2c3d479
config.json:4:19  uuid v7  018f3e5c-8a9b-7cde-8f01-23456789abcd  2024-05-03T12:10:03.291Z
config.json:6:13  refused (nil_or_max)  00000000-0000-0000-0000-000000000000  — the nil UUID: 128 zero bits, which RFC 9562 defines as naming nothing
2 identifiers in 1 file
2 runs refused
```

Columns count UTF-16 code units, as an editor does.

## Exit codes

They follow grep:

- **0**: identifiers found.
- **1**: none found.
- **2**: the question was malformed, a directory could not be walked, or
  `--strict` and something was refused or could not be read.

## For agents

`ids-le mcp` serves the Model Context Protocol on stdio with two tools:
`extract_ids` for a document's text and `ids_le_scan` for files and
directories. Hosts that want the document tool alone can install
[`ids-le-mcp`](https://www.npmjs.com/package/ids-le-mcp).

`ids-le --help` lists every option.

MIT licensed. Part of [letools](https://letools.dev/tools/ids-le).
