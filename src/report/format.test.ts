import { describe, expect, it } from 'vitest';
import { extract } from '../extract';
import { formatReport } from './format';

const CLOCK = { nowMs: 1_786_492_800_000 };

describe('the report', () => {
	it('says when a document holds no identifiers', () => {
		expect(formatReport({ file: 'a.md', format: 'text', rows: [] })).toContain(
			'No identifiers found.',
		);
	});

	it('shows the kind a refusal claims, a non-RFC variant, and the decode that caused it', () => {
		const rows = extract(
			'f47ac10b-58cc-4372-1567-0e02b2c3d479 7fffffff-ffff-7abc-8def-0123456789ab',
			'text',
			{ clock: CLOCK, kind: undefined },
		);
		const text = formatReport({ file: 'a.txt', format: 'text', rows });
		expect(text).toContain(
			'`f47ac10b-58cc-4372-1567-0e02b2c3d479` · uuid · ncs',
		);
		expect(text).toContain('· uuid · v7 · 6429-10-17T02:45:55.327Z');
		expect(text).toContain(
			'timestamp_implausible: the v7 time field decodes outside the plausible range',
		);
	});

	it('leads each row with its line and column, unless told not to', () => {
		const rows = extract('f47ac10b-58cc-4372-a567-0e02b2c3d479', 'text', {
			clock: CLOCK,
			kind: undefined,
		});
		const shown = formatReport({ file: 'a.txt', format: 'text', rows });
		const hidden = formatReport({
			file: 'a.txt',
			format: 'text',
			rows,
			positions: false,
		});
		expect(shown).toContain(
			'- **1:1** · `f47ac10b-58cc-4372-a567-0e02b2c3d479`',
		);
		expect(hidden).toContain('- `f47ac10b-58cc-4372-a567-0e02b2c3d479`');
		expect(hidden).not.toMatch(/\*\*\d+:\d+\*\*/);
		// Nothing but the position goes: the same rows, the same count.
		expect(hidden.split('\n').length).toBe(shown.split('\n').length);
	});

	it('keeps a backtick in a key from closing its code span', () => {
		const rows = extract(
			'{"a`id":"f47ac10b-58cc-4372-a567-0e02b2c3d479"}',
			'json',
			{ clock: CLOCK, kind: undefined },
		);
		expect(formatReport({ file: 'k.json', format: 'json', rows })).toContain(
			"key `a'id`",
		);
	});
});
