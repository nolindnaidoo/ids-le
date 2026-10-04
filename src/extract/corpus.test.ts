import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { extract, type Found } from './index';
import type { Reader } from './locate';

/**
 * The crate's `fixtures/extraction.json`, run through the port: the same checks
 * `crate/src/extract/corpus.rs` runs, against the same pinned clock.
 */
const FIXTURES = join(__dirname, '..', '..', 'crate', 'fixtures');
const corpus = JSON.parse(
	readFileSync(join(FIXTURES, 'extraction.json'), 'utf8'),
) as {
	now: number;
	documents: Array<{
		name: string;
		file: string;
		fileType: Reader;
		expected: Found[];
	}>;
	ambiguity: Array<{ value: string; reason: string }>;
	timestamps: Array<{ value: string; kind: string; timestamp: string }>;
};
const rows = (file: string, reader: Reader) =>
	extract(readFileSync(join(FIXTURES, 'documents', file), 'utf8'), reader, {
		clock: { nowMs: corpus.now },
		kind: undefined,
	});

describe('the shared corpus: documents', () => {
	for (const document of corpus.documents) {
		it(document.name, () => {
			expect(
				JSON.parse(JSON.stringify(rows(document.file, document.fileType))),
			).toEqual(document.expected);
		});
	}
});

describe('the shared corpus: refusals', () => {
	it('refuses every ambiguity case for the reason it pins, in order', () => {
		const found = rows('ambiguous.json', 'json');
		expect(found.map((row) => [row.value, row.refused])).toEqual(
			corpus.ambiguity.map((c) => [c.value, c.reason]),
		);
	});
});

describe('the shared corpus: timestamps', () => {
	it('decodes every pinned timestamp', () => {
		const found = rows('timestamps.json', 'json');
		expect(found.map((row) => [row.value, row.kind, row.timestamp])).toEqual(
			corpus.timestamps.map((c) => [c.value, c.kind, c.timestamp]),
		);
	});
});

describe('the shared corpus: context', () => {
	it('finds the same runs read as text, and names no field an identifier without keys', () => {
		const keyed = rows('context.json', 'json');
		const bare = rows('context.json', 'text');
		expect(bare.map((row) => row.value)).toEqual(keyed.map((row) => row.value));
		expect(bare.every((row) => row.key === undefined && !row.valid)).toBe(true);
	});
});
