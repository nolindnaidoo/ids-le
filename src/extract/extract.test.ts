import { describe, expect, it } from 'vitest';
import { extract, resolveFormat } from './index';
import { iso8601 } from './time';

const CLOCK = { nowMs: 1_786_492_800_000 };
const run = (text: string, reader: Parameters<typeof extract>[1]) =>
	extract(text, reader, { clock: CLOCK, kind: undefined });
const UUID = 'f47ac10b-58cc-4372-a567-0e02b2c3d479';

describe('positions', () => {
	it('counts columns in UTF-16 code units, as an editor does', () => {
		const [row] = run(`é😀 ${UUID}`, 'text');
		expect(row?.column).toBe(5);
	});
});

describe('YAML indentation is measured in bytes, as the crate measures it', () => {
	it('nests a line indented with U+3000 by its byte width', () => {
		// Two U+3000 are six bytes: deeper than the two-space `id`, so `ref` nests
		// under `id` in bytes, where a code-unit count would make them siblings.
		const text = `a:\n  id: x\n\u3000\u3000ref: ${UUID}\n`;
		expect(run(text, 'yaml')[0]?.key).toBe('a.id.ref');
	});
});

describe('time', () => {
	it('writes years outside 0000–9999 in the expanded form', () => {
		expect(iso8601(281_474_976_710_655)).toBe('+010889-08-02T05:31:50.655Z');
		expect(iso8601(-62_198_755_200_001)).toBe('-000002-12-31T23:59:59.999Z');
	});

	it('decodes a zeroed v1 clock to a day before the epoch, flooring as div_euclid does', () => {
		const [row] = run('00000000-0000-1000-8000-0123456789ab', 'text');
		expect(row?.timestamp).toBe('1582-10-15T00:00:00.000Z');
		expect(row?.refused).toBe('timestamp_implausible');
	});
});

describe('Snowflakes and ObjectIds need the document to name the field', () => {
	it('chooses the Discord epoch from the key path, and refuses an integer in prose', () => {
		expect(
			run('{"discord":{"id":"1536886938009600000"}}', 'json')[0]?.valid,
		).toBe(true);
		expect(run('the id is 1536886938009600000', 'text')).toEqual([]);
	});

	it('refuses 24 hex digits under a key that does not name an id', () => {
		const [row] = run('checksum: 6a7bb780a1b2c3d4e5f60718', 'yaml');
		expect(row?.refused).toBe('ambiguous_kind');
		expect(row?.kind).toBeNull();
	});
});

describe('a trailing comment lends no key', () => {
	it('stops the value region at a comment, so a hash after it is not named an ObjectId', () => {
		const [row] = run('_id = 1 # 6a7bb780a1b2c3d4e5f60718', 'toml');
		expect(row?.key).toBeUndefined();
		expect(row?.valid).toBe(false);
	});
});

describe('kind filter', () => {
	it('keeps refusals under any kind', () => {
		const rows = extract(
			`a: ${UUID}\nb: 5d41402abc4b2a76b9719d911017c592\n`,
			'yaml',
			{ clock: CLOCK, kind: 'ulid' },
		);
		expect(rows.map((row) => row.refused)).toEqual(['ambiguous_kind']);
	});
});

describe('format resolution', () => {
	it('resolves names, extensions and dotenv spellings, and reads .conf as text', () => {
		expect(resolveFormat('YML ', undefined)).toBe('yaml');
		expect(resolveFormat(undefined, '.env.local')).toBe('env');
		expect(resolveFormat(undefined, 'prod.env')).toBe('env');
		expect(resolveFormat(undefined, 'env.ts')).toBe('text');
		expect(resolveFormat(undefined, 'app.conf')).toBe('text');
		expect(resolveFormat('handwriting', 'x.tsv')).toBe('tsv');
	});
});
