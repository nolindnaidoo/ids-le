import { Readable } from 'node:stream';
import { describe, expect, it } from 'vitest';
import { kindOf, parse } from './args';
import { serve } from './mcp';
import { run, summary } from './run';
import { exitCode, describe as line, scanContent } from './scan';
import { FLAGS, USAGE } from './usage';

const NOW = 1_786_492_800_000;
const OPTIONS = { format: null, kind: undefined, nowMs: NOW };
const UUID = 'f47ac10b-58cc-4372-a567-0e02b2c3d479';
const NIL = '00000000-0000-0000-0000-000000000000';

describe('usage', () => {
	it('names every flag the parser accepts, and no other', () => {
		const named = new Set(
			USAGE.split(/\s+/)
				.filter((word) => word.startsWith('--'))
				.map((word) => word.replace(/[,.:;)]+$/, ''))
				.filter((word) => word !== '--version' && word !== '--help'),
		);
		expect([...named].sort()).toEqual([...FLAGS].sort());
	});
});

describe('parse', () => {
	it('reads a kind as the crate does, with Rust trim', () => {
		expect(kindOf(' ULID ')).toBe('ulid');
		expect(kindOf('uuid\u0085')).toBe('uuid');
		expect(() => kindOf('uuid﻿')).toThrow('is not a kind. Try one of');
	});

	it('refuses what the crate refuses, in its words', () => {
		expect(() => parse(['--format'])).toThrow('--format needs a format');
		expect(() => parse(['--kind'])).toThrow(
			'--kind needs one of uuid, ulid, nanoid, objectid, snowflake',
		);
		expect(() => parse(['--stdin', 'x'])).toThrow('takes no file arguments');
		expect(() => parse([])).toThrow('name a file or a directory to read');
	});
});

describe('exitCode', () => {
	it('follows grep, and fails a refusal only under --strict', () => {
		const found = scanContent(UUID, 'a', 'text', OPTIONS);
		const refused = scanContent(NIL, 'b', 'text', OPTIONS);
		expect(exitCode([found], false)).toBe(0);
		expect(exitCode([refused], false)).toBe(1);
		expect(exitCode([found, refused], true)).toBe(2);
	});
});

describe('describe', () => {
	it('lists a named row and a refused one as the crate does', () => {
		const report = scanContent(`${UUID}\n${NIL}`, 'a.txt', 'text', OPTIONS);
		const [named, nil] = report.ids;
		expect(named && line(report, named)).toBe(`a.txt:1:1  uuid v4  ${UUID}`);
		expect(nil && line(report, nil)).toMatch(
			/^a\.txt:2:1 {2}refused \(nil_or_max\) {2}0{8}-.* {2}— the nil UUID/,
		);
		expect(summary([report], 2)).toMatch(
			/1 identifier in 1 file\n1 run refused\n2 binary files skipped\n$/,
		);
	});
});

describe('run', () => {
	it('reads stdin as text, keyed by --format', async () => {
		const out: string[] = [];
		const code = await run(['--stdin', '--format', 'json'], {
			stdout: (text) => out.push(text),
			stderr: () => {},
			stdoutIsTerminal: false,
			readStdin: async () => Buffer.from(`{"a": {"b": "${UUID}"}}`),
			version: '9.9.9',
			nowMs: NOW,
		});
		expect(code).toBe(0);
		const report = JSON.parse(out.join(''));
		expect([report.file, report.format, report.ids[0].key]).toEqual([
			'<stdin>',
			'json',
			'a.b',
		]);
	});
});

describe('mcp', () => {
	it('offers both tools, keys sorted', async () => {
		const written: string[] = [];
		const request = { jsonrpc: '2.0', id: 1, method: 'tools/list' };
		await serve(
			Readable.from([Buffer.from(`${JSON.stringify(request)}\n`)]),
			(reply) => {
				written.push(reply);
				return undefined;
			},
			'9.9.9',
		);
		const reply = JSON.parse(written[0] ?? '{}');
		expect(
			reply.result.tools.map((tool: { name: string }) => tool.name),
		).toEqual(['extract_ids', 'ids_le_scan']);
		expect(written[0]?.startsWith('{"id":1,"jsonrpc":"2.0","result":')).toBe(
			true,
		);
	});
});

describe('mcp: the crate framing', () => {
	// The protocol around the tools answers as the extension's server and
	// the crate's answer, so a client cannot tell the three apart by what it
	// sent. Replies are parsed: key order is the crate's concern, not this one.
	async function session(
		lines: readonly (string | Uint8Array)[],
	): Promise<{ status: number; replies: unknown[] }> {
		const written: string[] = [];
		const input = Readable.from(lines.map((item) => Buffer.from(item)));
		const status = await serve(
			input,
			(line) => {
				written.push(line);
				return undefined;
			},
			'9.9.9',
		);
		return { status, replies: written.map((line) => JSON.parse(line)) };
	}

	it("answers a malformed frame in the extension server's words and serves the next line", async () => {
		// A frame that is not JSON, or not UTF-8, was dropped without a word,
		// and a client lost every request it then waited on.
		const { status, replies } = await session([
			'{"jsonrpc":"2.0","id":1,"method":"initialize"}\n',
			'{"jsonrpc":"2.0","method":"notifications/initialized"}\n',
			'not json\n',
			Uint8Array.from([0x7b, 0xff, 0x7d, 0x0a]),
			'   \n',
			'{"jsonrpc":"2.0","id":2,"method":"nope"}\n',
		]);
		expect(status).toBe(0);
		expect(replies).toEqual([
			{
				jsonrpc: '2.0',
				id: 1,
				result: {
					protocolVersion: '2025-06-18',
					capabilities: { tools: {} },
					serverInfo: { name: 'ids-le', version: '9.9.9' },
				},
			},
			{
				jsonrpc: '2.0',
				id: null,
				error: { code: -32700, message: 'Invalid JSON' },
			},
			{
				jsonrpc: '2.0',
				id: null,
				error: { code: -32700, message: 'Invalid JSON' },
			},
			{
				jsonrpc: '2.0',
				id: 2,
				error: { code: -32601, message: 'Unknown method: nope' },
			},
		]);
	});

	it('refuses malformed frames in the words the other two servers use, and a null id is a notification', async () => {
		const { replies } = await session([
			'{"id":1,"method":"ping"}\n',
			'{"jsonrpc":"1.0","id":2,"method":"ping"}\n',
			'[{"jsonrpc":"2.0","id":3,"method":"ping"}]\n',
			'{"jsonrpc":"2.0","id":null,"method":"ping"}\n',
			'{"jsonrpc":"2.0","id":4,"method":"notifications/initialized"}\n',
			'{"jsonrpc":"2.0","id":5,"method":"tools/call","params":5}\n',
			'{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"nope"}}\n',
			'{"jsonrpc":"2.0","id":7,"method":"tools/call"}\n',
			'{"jsonrpc":"2.0","id":8,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}\n',
		]);
		const refused = {
			jsonrpc: '2.0',
			id: null,
			error: { code: -32700, message: 'Not a JSON-RPC 2.0 request' },
		};
		expect(replies).toEqual([
			refused,
			refused,
			refused,
			{
				jsonrpc: '2.0',
				id: 4,
				error: {
					code: -32601,
					message: 'Unknown method: notifications/initialized',
				},
			},
			{
				jsonrpc: '2.0',
				id: 5,
				error: { code: -32602, message: 'tools/call requires a string `name`' },
			},
			{
				jsonrpc: '2.0',
				id: 6,
				error: { code: -32602, message: 'Unknown tool: nope' },
			},
			{
				jsonrpc: '2.0',
				id: 7,
				error: { code: -32602, message: 'tools/call requires a string `name`' },
			},
			{
				jsonrpc: '2.0',
				id: 8,
				result: {
					capabilities: { tools: {} },
					protocolVersion: '2025-06-18',
					serverInfo: { name: 'ids-le', version: '9.9.9' },
				},
			},
		]);
	});

	it('waits for the pipe to drain before reading the next frame', async () => {
		const lines: string[] = [];
		let drained = 0;
		const input = Readable.from([
			Buffer.from(
				'{"jsonrpc":"2.0","id":1,"method":"ping"}\n{"jsonrpc":"2.0","id":2,"method":"ping"}\n',
			),
		]);
		const write = (line: string) => {
			lines.push(line);
			return new Promise<void>((resolve) =>
				setTimeout(() => {
					drained += 1;
					resolve();
				}, 1),
			);
		};
		await serve(input, write, '9.9.9');
		expect(lines).toHaveLength(2);
		expect(drained).toBe(2);
	});
});
