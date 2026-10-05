# ids-le-mcp

<p align="center">
  <a href="https://marketplace.visualstudio.com/items?itemName=nolindnaidoo.ids-le">
    <img src="https://img.shields.io/badge/Install%20from-VS%20Code-blue?style=for-the-badge&logo=visualstudiocode" alt="Install from VS Code Marketplace" />
  </a>
  <a href="https://open-vsx.org/extension/nolindnaidoo/ids-le">
    <img src="https://img.shields.io/open-vsx/dt/nolindnaidoo/ids-le?style=for-the-badge&label=Open%20VSX&color=blue" alt="Open VSX downloads" />
  </a>
  <a href="https://www.npmjs.com/package/ids-le-mcp">
    <img src="https://img.shields.io/npm/v/ids-le-mcp?style=for-the-badge&label=MCP%20server&color=blue&logo=npm" alt="ids-le-mcp on npm" />
  </a>
  <a href="https://letools.dev/tools/ids-le">
    <img src="https://img.shields.io/badge/LE%20Tools-letools.dev-blue?style=for-the-badge" alt="LE Tools" />
  </a>
</p>

An [MCP](https://modelcontextprotocol.io) server that finds every identifier
in a document — UUID at every version, ULID, NanoID, MongoDB ObjectId and
Snowflake — with its line, column and key path, whether it is valid, and the
time it was minted where it carries one: the extraction engine behind the
[IDs-LE](https://letools.dev/tools/ids-le) editor extension, exposed as a tool
an agent can call.

**A run it cannot name honestly comes back as a row with the reason, never
dropped.** Thirty-two hex digits are an unhyphenated UUID and an MD5 digest in
equal measure, so that run is returned with `valid: false` and
`ambiguous_kind`, not guessed at. An ObjectId and a Snowflake have nothing in
them to validate, so they are named only under a key that says the field holds
an id.

No dependencies, no network calls, no filesystem access. Content goes in,
structured results come out.

## Use it

Point any MCP host at `npx ids-le-mcp`.

**Claude Code**

```bash
claude mcp add ids-le -- npx -y ids-le-mcp
```

**Anything with a JSON config** — Cursor, Windsurf, Claude Desktop:

```json
{
  "mcpServers": {
    "ids-le": {
      "command": "npx",
      "args": ["-y", "ids-le-mcp"]
    }
  }
}
```

**VS Code** needs nothing here. Install the extension instead — it
carries this server and registers it for you:
[VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=nolindnaidoo.ids-le)
· [Open VSX](https://open-vsx.org/extension/nolindnaidoo/ids-le)

**No Node?** The same `extract_ids` tool ships in a static Rust binary:
`cargo install ids-le`, then `ids-le mcp`
([crates.io](https://crates.io/crates/ids-le)). The two servers answer
identically — one corpus runs against both, and a differential test feeds both
thousands of generated documents in every format and compares every answer.
The binary additionally offers `ids_le_scan`, which walks a tree; **this server
reads no files**.

Prefer a global install to `npx` on every launch:

```bash
npm install -g ids-le-mcp
```

No environment variables, no API key, no configuration of its own. To check it
before wiring it into anything:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | npx -y ids-le-mcp
```

If that prints the tool name, the server works.

## The tool

### `extract_ids`

| argument | type | |
|---|---|---|
| `content` | string | **required.** The document text to scan. |
| `format` | string | `json`, `yaml`, `csv`, `tsv`, `toml`, `ini`, `env` or `text`. Optional — it decides only the key paths, never which runs are found. |
| `filename` | string | Used to infer the format when `format` is absent, e.g. `config.toml`. |
| `kind` | string | Return only `uuid`, `ulid`, `nanoid`, `objectid` or `snowflake`. A run that could not be assigned a kind is then not returned; omit it for the complete answer. |
| `maxResults` | number | Default `500`, ceiling `5000`. |

Every row carries its raw text, a 1-based line and UTF-16 column, the key path
where the format has one, and `valid`. A named row adds what was decoded; a
refused row adds `refused` and a `detail` sentence:

```json
{
  "ok": true,
  "data": {
    "ids": [
      {
        "kind": "uuid",
        "value": "019ff344-cc00-7abc-8def-0123456789ab",
        "line": 3,
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
        "line": 5,
        "column": 14,
        "key": "digest",
        "valid": false,
        "refused": "ambiguous_kind",
        "detail": "32 hex digits are an unhyphenated UUID and an MD5 digest in equal measure; nothing in this document chooses between them"
      }
    ],
    "fileType": "json",
    "named": 1,
    "refused": 1
  },
  "diagnostics": [
    {
      "severity": "warning",
      "code": "refused",
      "message": "1 run(s) could not be named; each is returned with `valid: false` and a reason"
    }
  ],
  "meta": {
    "tool": "extract_ids",
    "count": 2,
    "truncated": false
  }
}
```

`ok` means the scan ran, not that every run was named.

## Also in the MCP registry

`io.github.nolindnaidoo/ids-le` —
[registry.modelcontextprotocol.io](https://registry.modelcontextprotocol.io)

## Twelve more like it

One tool each, same shape: content in, structured data out, no network and no
filesystem. Every one is on npm as `<name>-mcp` and in the MCP registry as
`io.github.nolindnaidoo/<name>`.

| Package | Tool | Does |
|---|---|---|
| [`urls-le-mcp`](https://www.npmjs.com/package/urls-le-mcp) | `extract_urls` | URLs, with protocol and position |
| [`colors-le-mcp`](https://www.npmjs.com/package/colors-le-mcp) | `extract_colors` | colors from stylesheets and code |
| [`dates-le-mcp`](https://www.npmjs.com/package/dates-le-mcp) | `extract_dates` | dates and timestamps |
| [`numbers-le-mcp`](https://www.npmjs.com/package/numbers-le-mcp) | `extract_numbers` | numeric values |
| [`paths-le-mcp`](https://www.npmjs.com/package/paths-le-mcp) | `extract_paths` | file and directory paths |
| [`string-le-mcp`](https://www.npmjs.com/package/string-le-mcp) | `extract_strings` | string values |
| [`regex-le-mcp`](https://www.npmjs.com/package/regex-le-mcp) | `extract_patterns` | regexes, with a ReDoS verdict |
| [`secrets-le-mcp`](https://www.npmjs.com/package/secrets-le-mcp) | `detect_secrets` | credentials, masked — never the value |
| [`envsync-le-mcp`](https://www.npmjs.com/package/envsync-le-mcp) | `compare_env_files` | dotenv key drift, names only |
| [`scrape-le-mcp`](https://www.npmjs.com/package/scrape-le-mcp) | `analyze_robots_txt` | whether a path may be crawled |
| [`unicode-le-mcp`](https://www.npmjs.com/package/unicode-le-mcp) | `detect_unicode_risks` | Unicode that hides meaning, as codepoints |
| [`i18n-le-mcp`](https://www.npmjs.com/package/i18n-le-mcp) | `check_catalogues` | translation catalogues, keys only |

Every tool in the family, one page: **[letools.dev](https://letools.dev)**

## Built by

**[Nolin Naidoo](https://nolindnaidoo.com)** — Chief Engineer, AI/ML & Platform
Architecture. [nolindnaidoo.com](https://nolindnaidoo.com) ·
[GitHub](https://github.com/nolindnaidoo) ·
[LinkedIn](https://www.linkedin.com/in/nolindnaidoo/)

### Also from the same workshop

Twelve Rust tools built the same way: small, single-purpose, and driven by a
machine rather than a person. pixelcoords and pixelactions make up one loop —
pixelcoords answers *where*, pixelactions *acts* there. The ten LE crates are
the terminal half of the extensions they sit in: the same detection, held to
the extension's own corpus, and an exit code instead of a results editor.

| | | |
|---|---|---|
| **[pixelcoords](https://github.com/nolindnaidoo/pixelcoords)** | Freeze your screen, mark regions, get pixel-exact coordinates and crops | [site](https://pixelcoords.dev) · [crates.io](https://crates.io/crates/pixelcoords) · [docs.rs](https://docs.rs/pixelcoords) |
| **[pixelactions](https://github.com/nolindnaidoo/pixelactions)** | Consume human-verified coordinates, perform the interaction, confirm it landed | [site](https://pixelactions.dev) · [crates.io](https://crates.io/crates/pixelactions) · [docs.rs](https://docs.rs/pixelactions) |
| **[paths-le](https://github.com/nolindnaidoo/paths-le/tree/main/crate)** | Find every path in a codebase and report whether it still points at anything | [crates.io](https://crates.io/crates/paths-le) |
| **[secrets-le](https://github.com/nolindnaidoo/secrets-le/tree/main/crate)** | Find hardcoded credentials, and never print one | [crates.io](https://crates.io/crates/secrets-le) |
| **[urls-le](https://github.com/nolindnaidoo/urls-le/tree/main/crate)** | Extract every URL from a codebase, with its protocol and exact position | [crates.io](https://crates.io/crates/urls-le) |
| **[regex-le](https://github.com/nolindnaidoo/regex-le/tree/main/crate)** | Find every regex in a codebase and report which can be driven into catastrophic backtracking | [crates.io](https://crates.io/crates/regex-le) |
| **[string-le](https://github.com/nolindnaidoo/string-le/tree/main/crate)** | Get every string in a codebase out where a person can read them | [crates.io](https://crates.io/crates/string-le) |
| **[numbers-le](https://github.com/nolindnaidoo/numbers-le/tree/main/crate)** | Find every hardcoded number in a codebase so a person can check them | [crates.io](https://crates.io/crates/numbers-le) |
| **[envsync-le](https://github.com/nolindnaidoo/envsync-le/tree/main/crate)** | Compare the dotenv files in a tree and say which keys are missing from which | [crates.io](https://crates.io/crates/envsync-le) |
| **[colors-le](https://github.com/nolindnaidoo/colors-le/tree/main/crate)** | Find every colour in a codebase, and say which are not in your palette | [crates.io](https://crates.io/crates/colors-le) |
| **[dates-le](https://github.com/nolindnaidoo/dates-le/tree/main/crate)** | Extract every date and timestamp, and the exact instant each one resolves to | [crates.io](https://crates.io/crates/dates-le) |
| **[scrape-le](https://github.com/nolindnaidoo/scrape-le/tree/main/crate)** | Check whether a page is scrapeable before the scraper is written | [crates.io](https://crates.io/crates/scrape-le) |

## Licence

MIT © [Nolin Naidoo](https://nolindnaidoo.com)
