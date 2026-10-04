import {
	counts,
	extract,
	KIND_NAMES,
	type Kind,
	resolveFormat,
	SUPPORTED_FORMATS,
} from '../extract';
import {
	capped,
	DEFAULT_MAX_RESULTS,
	type Diagnostic,
	MAX_MAX_RESULTS,
	readMaxResults,
} from './envelope';
import type { ToolDefinition } from './transport';

/**
 * The tool this server exposes: `extract_ids`, which the crate's server offers
 * too. One name, one schema, two implementations — a caller must get the same
 * answer whichever it reaches, so the definition below is the crate's, word
 * for word, and `crate/fixtures/mcp-extract-ids.json` pins the answers on both
 * sides.
 *
 * **Refusals come back as rows.** An agent that received only the identifiers
 * this tool was willing to name would conclude a document was clean when
 * nothing in it could be named. Now is read once per call, here, never inside
 * the analysis.
 */

const DESCRIPTION =
	"Extract every identifier from a document — UUID (all versions), ULID, NanoID, MongoDB ObjectId and Snowflake — with its raw text, its line and column, the document's key path for it, and its validity. Where the identifier embeds a time (UUID v1/v6/v7, ULID, ObjectId, Snowflake) the decoded instant is returned as an ISO-8601 UTC string. Parses JSON, YAML, CSV, TOML, INI and dotenv for key paths; anything else is read as text, so a format is optional. A run that cannot be named is returned as a row with `valid: false` and a named reason, never dropped.";

function readKind(args: Record<string, unknown>): Kind | undefined {
	if (!Object.hasOwn(args, 'kind')) return undefined;
	const raw = args.kind;
	if (typeof raw !== 'string') throw new Error('kind must be a string');
	const name = raw.trim().toLowerCase();
	const kind = KIND_NAMES.find((candidate) => candidate === name);
	if (kind === undefined)
		throw new Error(
			`${raw} is not a kind; it is one of ${KIND_NAMES.join(', ')}`,
		);
	return kind;
}

function extractIds(args: Record<string, unknown>): Promise<unknown> {
	// The crate's order: content, then the cap, then the kind.
	if (typeof args.content !== 'string')
		throw new Error('content is required and must be a string');
	const content = args.content;
	const maxResults = readMaxResults(args);
	const kind = readKind(args);
	const format = resolveFormat(
		typeof args.format === 'string' ? args.format : undefined,
		typeof args.filename === 'string' ? args.filename : undefined,
	);

	const rows = extract(content, format, { clock: { nowMs: Date.now() }, kind });
	const [named, refused] = counts(rows);
	const { items, truncated } = capped(rows, maxResults);

	// A refusal is not an error — the scan ran — but it is said out loud, so a
	// model counting only the named rows does not understate the document.
	const diagnostics: Diagnostic[] =
		refused > 0
			? [
					{
						severity: 'warning',
						code: 'refused',
						message: `${refused} run(s) could not be named; each is returned with \`valid: false\` and a reason`,
					},
				]
			: [];

	return Promise.resolve({
		ok: true,
		data: { ids: items, fileType: format, named, refused },
		diagnostics,
		meta: { tool: 'extract_ids', count: items.length, truncated },
	});
}

export const TOOLS: readonly ToolDefinition[] = Object.freeze([
	Object.freeze({
		name: 'extract_ids',
		description: DESCRIPTION,
		inputSchema: {
			type: 'object',
			properties: {
				content: { type: 'string', description: 'The document text to scan.' },
				format: {
					type: 'string',
					enum: SUPPORTED_FORMATS,
					description:
						'Document format. Optional — an unrecognised or absent format reads the text directly and returns findings without key paths.',
				},
				filename: {
					type: 'string',
					description:
						'Filename used to infer the format when `format` is absent, e.g. "config.toml".',
				},
				kind: {
					type: 'string',
					enum: KIND_NAMES,
					description:
						'Return only one kind. This narrows the report after the analysis, so a run that could not be assigned a kind is not returned at all; omit it for the complete answer.',
				},
				maxResults: {
					type: 'integer',
					minimum: 1,
					maximum: MAX_MAX_RESULTS,
					default: DEFAULT_MAX_RESULTS,
					description: `Cap on returned rows (default ${DEFAULT_MAX_RESULTS}). meta.truncated reports whether any were dropped.`,
				},
			},
			required: ['content'],
			additionalProperties: false,
		},
		handler: extractIds,
	}),
]);
