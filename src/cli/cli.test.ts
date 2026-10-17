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
			(reply) => written.push(reply),
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
