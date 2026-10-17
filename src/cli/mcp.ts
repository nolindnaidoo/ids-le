/**
 * `ids-le mcp`: the crate's MCP server (`crate/src/mcp/`), not the
 * extension's. It offers `ids_le_scan` beside the shared `extract_ids`,
 * answers in the crate's words, and writes every reply with its keys sorted,
 * as a `serde_json::Value` without `preserve_order` does.
 */

import { KIND_NAMES, type Kind, resolveFormat } from '../extract';
import { trim } from '../extract/text';
import { sortedKeys } from '../mcp/envelope';
import { TOOLS } from '../mcp/tools';
import { requests } from './frames';
import { type FileReport, scanFile } from './scan';
import { collect } from './walk';

const PROTOCOL_VERSION = '2025-06-18';
const INVALID_PARAMS = -32602;
const METHOD_NOT_FOUND = -32601;

type Handled =
	| { readonly result: unknown }
	| { readonly error: { readonly code: number; readonly message: string } };

const SHARED = sharedTool('extract_ids');

const SCAN_DEFINITION = {
	name: 'ids_le_scan',
	description:
		"Extract every identifier from files or directories, with the file it came from, its line and column, the document's key path for it, and the decoded time where the identifier carries one. Reads the filesystem; never writes to it. A run that cannot be named is returned as a row with `valid: false` and a named reason, never dropped.",
	inputSchema: {
		type: 'object',
		properties: {
			path: { type: 'string', description: 'a file or directory to read' },
			paths: {
				type: 'array',
				items: { type: 'string' },
				description: 'several files or directories, instead of `path`',
			},
			format: {
				type: 'string',
				description:
					'Force a format for every file instead of inferring one per file name. An unrecognised name reads the text directly, without key paths.',
			},
			kind: {
				type: 'string',
				enum: [...KIND_NAMES],
				description:
					'Return only one kind. This narrows the report after the analysis; omit it for the complete answer.',
			},
			hidden: {
				type: 'boolean',
				default: false,
				description:
					'Walk hidden files and directories too, which is where a dotenv file lives.',
			},
			ignored: {
				type: 'boolean',
				default: false,
				description: 'Walk files excluded by .gitignore too.',
			},
		},
		anyOf: [{ required: ['path'] }, { required: ['paths'] }],
		additionalProperties: false,
	},
};

/** The shared tool is the extension's, definition and handler both. */
function sharedTool(name: string) {
	const tool = TOOLS.find((candidate) => candidate.name === name);
	if (!tool) throw new Error(`the extension offers no tool named ${name}`);
	return tool;
}

/**
 * Serves until stdin closes, one request at a time, in order, over the
 * crate's framing in `frames.ts`.
 */
export async function serve(
	input: AsyncIterable<Uint8Array>,
	write: (line: string) => void,
	version: string,
): Promise<number> {
	for await (const request of requests(input)) {
		const response = await handle(request, version);
		if (response !== undefined) write(`${response}\n`);
	}
	return 0;
}

function field(value: unknown, name: string): unknown {
	if (typeof value !== 'object' || value === null || Array.isArray(value)) {
		return undefined;
	}
	return Object.hasOwn(value, name)
		? (value as Record<string, unknown>)[name]
		: undefined;
}

async function handle(
	request: unknown,
	version: string,
): Promise<string | undefined> {
	const id = field(request, 'id');
	const method = field(request, 'method');
	if (typeof method !== 'string') return undefined;
	// A notification carries no id and gets no reply.
	if (id === undefined) return undefined;

	const handled = await dispatch(method, field(request, 'params'), version);
	return JSON.stringify(sortedKeys({ jsonrpc: '2.0', id, ...handled }));
}

async function dispatch(
	method: string,
	params: unknown,
	version: string,
): Promise<Handled> {
	if (method === 'initialize') {
		return {
			result: {
				protocolVersion: PROTOCOL_VERSION,
				capabilities: { tools: {} },
				serverInfo: { name: 'ids-le', version },
			},
		};
	}
	if (method === 'tools/list') {
		return {
			result: {
				tools: [
					{
						name: SHARED.name,
						description: SHARED.description,
						inputSchema: SHARED.inputSchema,
					},
					SCAN_DEFINITION,
				],
			},
		};
	}
	if (method === 'tools/call') return callTool(params);
	if (method === 'ping') return { result: {} };
	return {
		error: {
			code: METHOD_NOT_FOUND,
			message: `this server does not implement ${method}`,
		},
	};
}

/**
 * Protocol failures are JSON-RPC errors. A tool that fails on its arguments
 * returns a result carrying `isError`, so a model reads the reason rather than
 * concluding the server is broken.
 */
async function callTool(params: unknown): Promise<Handled> {
	if (params === undefined) {
		return {
			error: { code: INVALID_PARAMS, message: 'no tool call was supplied' },
		};
	}
	const name = field(params, 'name');
	if (typeof name !== 'string') {
		return {
			error: { code: INVALID_PARAMS, message: 'the tool call named no tool' },
		};
	}
	// serde_json answers `get` on anything but an object with nothing, so
	// arguments that are not an object read as an empty one.
	const raw = field(params, 'arguments');
	const args = isObject(raw) ? raw : {};
	if (name === 'extract_ids') {
		return { result: await outcome(() => SHARED.handler(args)) };
	}
	if (name === 'ids_le_scan') {
		return { result: await outcome(async () => scan(args)) };
	}
	return {
		error: {
			code: INVALID_PARAMS,
			message: `this server offers no tool named ${name}`,
		},
	};
}

function isObject(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

async function outcome(tool: () => Promise<unknown>): Promise<unknown> {
	let envelope: unknown;
	try {
		envelope = sortedKeys(await tool());
	} catch (error) {
		const message = error instanceof Error ? error.message : String(error);
		return { content: [{ type: 'text', text: message }], isError: true };
	}
	return {
		content: [{ type: 'text', text: JSON.stringify(envelope, null, 2) }],
		structuredContent: envelope,
		isError: false,
	};
}

/** The crate's `scan::run`: paths, then the kind, then the walk. */
function scan(args: Record<string, unknown>): unknown {
	const inputs = requestedPaths(args);
	const format =
		typeof args.format === 'string'
			? resolveFormat(args.format, undefined)
			: null;
	const options = { format, kind: requestedKind(args), nowMs: Date.now() };
	const files = collect(inputs, {
		hidden: args.hidden === true,
		respectIgnore: args.ignored !== true,
	});
	const read: FileReport[] = [];
	for (const file of files) {
		const scanned = scanFile(file, options);
		if (scanned) read.push(scanned);
	}
	const ids = read.reduce((total, report) => total + report.summary.ids, 0);
	const refused = read.reduce(
		(total, report) => total + report.summary.refused,
		0,
	);
	const diagnostics = read
		.filter((report) => report.diagnostics.length > 0)
		.map((report) => ({
			severity: 'warning',
			code: 'unreadable',
			message: `${report.file} could not be read, so this scan does not cover it`,
		}));
	if (refused > 0) {
		diagnostics.push({
			severity: 'warning',
			code: 'refused',
			message: `${refused} run(s) could not be named; each is in its file's report with \`valid: false\` and a reason`,
		});
	}
	return {
		ok: true,
		data: {
			reports: read,
			ids,
			refused,
			binaryFiles: files.length - read.length,
		},
		diagnostics,
		meta: { tool: 'ids_le_scan', count: read.length, truncated: false },
	};
}

/** The crate's `requested_kind`: Rust's trim, lowercase, refused by name. */
function requestedKind(args: Record<string, unknown>): Kind | undefined {
	if (!Object.hasOwn(args, 'kind')) return undefined;
	const raw = args.kind;
	if (typeof raw !== 'string') throw new Error('kind must be a string');
	const name = trim(raw).toLowerCase();
	const kind = KIND_NAMES.find((candidate) => candidate === name);
	if (kind === undefined) {
		throw new Error(
			`${raw} is not a kind; it is one of ${KIND_NAMES.join(', ')}`,
		);
	}
	return kind;
}

function requestedPaths(args: Record<string, unknown>): readonly string[] {
	if (typeof args.path === 'string') return [args.path];
	if (Array.isArray(args.paths)) {
		const paths = args.paths.filter(
			(item): item is string => typeof item === 'string',
		);
		if (paths.length === 0) throw new Error('the list of paths was empty');
		return paths;
	}
	throw new Error('no file or directory was supplied to read');
}
