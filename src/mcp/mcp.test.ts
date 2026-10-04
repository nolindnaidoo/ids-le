import { EventEmitter } from 'node:events';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { KIND_NAMES, SUPPORTED_FORMATS } from '../extract';
import { capped, readMaxResults } from './envelope';
import { TOOLS } from './tools';
import { createResponder, serve } from './transport';

/**
 * The MCP layer: the envelope, the one tool, and the protocol. The tool is
 * shared with the crate's server, so its answers are pinned by the crate's own
 * corpus here as well as there.
 */
const CORPUS = join(__dirname, '..', '..', 'crate', 'fixtures');
const tool = TOOLS[0] as (typeof TOOLS)[number];
const call = async (args: Record<string, unknown>) =>
	(await tool.handler(args)) as Record<string, any>;

describe('envelope', () => {
	it('reports truncation honestly when it drops items', () => {
		expect(capped([1, 2, 3], 2)).toEqual({ items: [1, 2], truncated: true });
		expect(capped([1, 2], 5)).toEqual({ items: [1, 2], truncated: false });
	});

	it('rejects a maxResults a tool cannot honour, and clamps an oversized one', () => {
		expect(() => readMaxResults({ maxResults: 0 })).toThrow(/positive integer/);
		expect(() => readMaxResults({ maxResults: 1.5 })).toThrow();
		expect(readMaxResults({ maxResults: 999999 })).toBe(5000);
	});
});

describe('tool table', () => {
	it("pins the tool name and offers exactly the engine's kinds and formats", () => {
		expect(TOOLS.map((t) => t.name)).toEqual(['extract_ids']);
		const properties = tool.inputSchema.properties as Record<
			string,
			{ enum?: readonly string[] }
		>;
		expect(properties.kind?.enum).toEqual(KIND_NAMES);
		expect(properties.format?.enum).toEqual(SUPPORTED_FORMATS);
	});
});

interface CorpusCase {
	readonly name: string;
	readonly file?: string;
	readonly content?: string;
	readonly arguments: Record<string, unknown>;
	readonly expected?: Record<string, unknown>;
	readonly expectedError?: string;
}

describe('extract_ids: the shared corpus', () => {
	const cases = JSON.parse(
		readFileSync(join(CORPUS, 'mcp-extract-ids.json'), 'utf8'),
	) as CorpusCase[];
	for (const testCase of cases) {
		it(testCase.name, async () => {
			const args: Record<string, unknown> = { ...testCase.arguments };
			if (testCase.file)
				args.content = readFileSync(
					join(CORPUS, 'documents', testCase.file),
					'utf8',
				);
			if (testCase.content !== undefined) args.content = testCase.content;
			if (testCase.expectedError !== undefined) {
				await expect(call(args)).rejects.toThrow(testCase.expectedError);
				return;
			}
			const answer = JSON.parse(JSON.stringify(await call(args)));
			for (const [key, value] of Object.entries(testCase.expected ?? {}))
				expect(answer[key]).toEqual(value);
		});
	}
});

describe('extract_ids: arguments', () => {
	it("refuses each malformed argument by name, in the crate's order", async () => {
		await expect(call({})).rejects.toThrow(
			'content is required and must be a string',
		);
		await expect(
			call({ content: 'x', maxResults: 0, kind: 'guid' }),
		).rejects.toThrow(/positive integer/);
		await expect(call({ content: 'x', kind: 7 })).rejects.toThrow(
			'kind must be a string',
		);
		await expect(call({ content: 'x', kind: ' UUID ' })).resolves.toMatchObject(
			{ ok: true },
		);
	});

	it('says out loud how many runs could not be named, and stays ok', async () => {
		const answer = await call({
			content: 'a 5d41402abc4b2a76b9719d911017c592 b',
		});
		expect(answer.ok).toBe(true);
		expect(answer.diagnostics).toEqual([
			{
				severity: 'warning',
				code: 'refused',
				message:
					'1 run(s) could not be named; each is returned with `valid: false` and a reason',
			},
		]);
	});
});

describe('protocol', () => {
	const respond = createResponder({ name: 'ids-le', version: '1.0.0' }, TOOLS);

	it('echoes the protocol version the client asked for', async () => {
		const reply = await respond({
			jsonrpc: '2.0',
			id: 1,
			method: 'initialize',
			params: { protocolVersion: '2024-11-05' },
		});
		expect(reply?.result?.protocolVersion).toBe('2024-11-05');
		expect(reply?.result?.serverInfo).toEqual({
			name: 'ids-le',
			version: '1.0.0',
		});
	});

	it('does not reply to a notification', async () => {
		// A reply to a notification is the classic way to wedge a client.
		expect(
			await respond({ jsonrpc: '2.0', method: 'notifications/initialized' }),
		).toBeNull();
	});

	it('reports an unknown method as a JSON-RPC error', async () => {
		const reply = await respond({ jsonrpc: '2.0', id: 2, method: 'nope' });
		expect(reply?.error?.code).toBe(-32601);
	});

	it('reports an unknown tool without killing the connection', async () => {
		const reply = await respond({
			jsonrpc: '2.0',
			id: 3,
			method: 'tools/call',
			params: { name: 'no_such_tool', arguments: {} },
		});
		expect(reply?.error?.code).toBe(-32602);
	});

	it('returns a tool failure as a result, not a protocol error', async () => {
		// A model can read an isError result and correct itself; a JSON-RPC error
		// reads as "the server is broken".
		const reply = await respond({
			jsonrpc: '2.0',
			id: 4,
			method: 'tools/call',
			params: { name: 'extract_ids', arguments: {} },
		});
		expect(reply?.error).toBeUndefined();
		expect(reply?.result?.isError).toBe(true);
	});
});

describe('serve: the stdio loop', () => {
	/** A fake stdin/stdout pair so the loop can be driven without a process. */
	function harness() {
		const input = new EventEmitter() as EventEmitter & {
			setEncoding?: (e: string) => void;
		};
		const written: string[] = [];
		const output = {
			write: (chunk: string) => {
				written.push(chunk);
				return true;
			},
		};
		serve(
			{ name: 'ids-le', version: '1.0.0' },
			TOOLS,
			input as never,
			output as never,
		);
		const replies = () =>
			written
				.join('')
				.split('\n')
				.filter(Boolean)
				.map((l) => JSON.parse(l));
		return { input, replies };
	}

	const settle = () => new Promise((r) => setTimeout(r, 20));

	it('answers a request delivered as one line', async () => {
		const { input, replies } = harness();
		input.emit('data', '{"jsonrpc":"2.0","id":1,"method":"tools/list"}\n');
		await settle();
		expect(replies()[0]?.result?.tools).toHaveLength(1);
	});

	it('reassembles a request split across chunks', async () => {
		// stdin delivers whatever the OS gives it; a request arriving in two
		// pieces must not be dropped or double-parsed.
		const { input, replies } = harness();
		input.emit('data', '{"jsonrpc":"2.0","id":2,"me');
		input.emit('data', 'thod":"ping"}\n');
		await settle();
		expect(replies()[0]?.id).toBe(2);
	});

	it('handles several requests in one chunk', async () => {
		const { input, replies } = harness();
		input.emit(
			'data',
			'{"jsonrpc":"2.0","id":3,"method":"ping"}\n{"jsonrpc":"2.0","id":4,"method":"ping"}\n',
		);
		await settle();
		expect(replies().map((r) => r.id)).toEqual([3, 4]);
	});

	it('reports malformed JSON without dying', async () => {
		// One bad line from a client must not take the server down for everyone.
		const { input, replies } = harness();
		input.emit('data', 'not json at all\n');
		input.emit('data', '{"jsonrpc":"2.0","id":5,"method":"ping"}\n');
		await settle();
		expect(replies()[0]?.error?.code).toBe(-32700);
		expect(replies()[1]?.id).toBe(5);
	});

	it('rejects a payload that is not a JSON-RPC request', async () => {
		const { input, replies } = harness();
		input.emit('data', '{"hello":"world"}\n');
		await settle();
		expect(replies()[0]?.error?.code).toBe(-32700);
	});

	it('ignores blank lines', async () => {
		const { input, replies } = harness();
		input.emit('data', '\n\n{"jsonrpc":"2.0","id":6,"method":"ping"}\n');
		await settle();
		expect(replies()).toHaveLength(1);
	});

	it('writes nothing for a notification', async () => {
		const { input, replies } = harness();
		input.emit(
			'data',
			'{"jsonrpc":"2.0","method":"notifications/initialized"}\n',
		);
		await settle();
		expect(replies()).toHaveLength(0);
	});
});
