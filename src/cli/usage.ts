/**
 * The command line's help text and the flags it names.
 *
 * Word for word the Rust CLI's, because both install as `ids-le` and a
 * person reading `--help` cannot tell which one they have.
 * `scripts/check-cli-parity.ts` compares the two outputs, and `cli.test.ts`
 * holds FLAGS equal to the flags this text names.
 */

export const USAGE = `usage: ids-le [options] <file|dir>...
       ids-le [options] --stdin [--format <format>]
       ids-le mcp
       ids-le --version | --help

Finds every identifier in a tree — UUID, ULID, NanoID, MongoDB ObjectId,
Snowflake — reports where it is and what it is, and decodes the time
inside the ones that carry one. JSON, YAML, CSV, TOML, INI and dotenv
also give each finding the document's own key path; anything else is read
as text and reported without one.

It refuses rather than guesses. A run that fits two schemes, a UUID whose
variant bits are wrong, the nil UUID, a v4 that is plainly not random, a
timestamp before 1990 — each is a row in the report with a named reason,
never a dropped row and never a silent answer.

One JSON report per line on stdout when it is piped or redirected, every
row listed on stderr.

Options:
  --kind <kind>        report only one of uuid, ulid, nanoid, objectid,
                       snowflake
  --format <format>    force a format instead of inferring it from the
                       file name; an unknown name falls back to a text
                       read rather than failing
  --strict             exit 2 if anything was refused or any file could
                       not be read, rather than reporting it and carrying
                       on
  --stdin              read one document from stdin
  --hidden             walk hidden files and directories too
  --no-ignore          walk files that .gitignore excludes

A binary file — a NUL byte in its first 8 KiB, ripgrep's own test — is
never a text candidate: it produces no report line, is counted on stderr,
and never fails the run. A file that looked like text and could not be
read is named on stderr, carried in the report, and does not fail the run
by itself.

Exit codes follow grep: 0 identifiers found · 1 none found · 2 malformed
question. A refusal is an answer, not an error, and does not move the
exit code unless --strict.`;

export const FLAGS: readonly string[] = Object.freeze([
	'--kind',
	'--format',
	'--strict',
	'--stdin',
	'--hidden',
	'--no-ignore',
]);
