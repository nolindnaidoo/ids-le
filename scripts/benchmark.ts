/**
 * Measure real throughput. Run with `bun run benchmark`.
 *
 * Numbers are machine-specific, so the host is recorded alongside them and
 * they are never asserted in CI. Inputs are generated rather than checked in so
 * the sizes are explicit.
 */
import { cpus, totalmem } from 'node:os';
import { extract } from '../src/extract';
import type { Reader } from '../src/extract/locate';

const CLOCK = { nowMs: 1_786_492_800_000 };
const UUID = 'f47ac10b-58cc-4372-a567-0e02b2c3d479';
const V7 = '019ff344-cc00-7abc-8def-0123456789ab';

const CASES: ReadonlyArray<{ label: string; reader: Reader; build: () => string }> = [
	{
		label: 'JSON records',
		reader: 'json',
		build: () =>
			JSON.stringify(
				Array.from({ length: 20_000 }, (_, i) => ({ id: i % 2 ? UUID : V7, ownerId: '6a7bb780a1b2c3d4e5f60718', note: `row ${i}` })),
				null,
				2,
			),
	},
	{
		label: 'Application log',
		reader: 'text',
		build: () =>
			Array.from({ length: 40_000 }, (_, i) => `2026-08-12T00:00:${String(i % 60).padStart(2, '0')}Z request ${V7} handled in ${i % 900}ms`).join('\n'),
	},
	{
		label: 'CSV export',
		reader: 'csv',
		build: () => ['user_id,session,digest', ...Array.from({ length: 30_000 }, () => `${UUID},01KZSM9K00ABCDEFGH12345678,5d41402abc4b2a76b9719d911017c592`)].join('\n'),
	},
];

const WARMUP = 2;
const RUNS = 7;
const median = (xs: readonly number[]) => {
	const s = [...xs].sort((a, b) => a - b);
	const mid = Math.floor(s.length / 2);
	return s.length % 2 ? (s[mid] as number) : ((s[mid - 1] as number) + (s[mid] as number)) / 2;
};

const results: Array<Record<string, unknown>> = [];
for (const c of CASES) {
	const content = c.build();
	const bytes = Buffer.byteLength(content, 'utf8');
	const run = () => extract(content, c.reader, { clock: CLOCK, kind: undefined }).length;
	for (let i = 0; i < WARMUP; i++) run();
	const durations: number[] = [];
	let count = 0;
	for (let i = 0; i < RUNS; i++) {
		const t0 = performance.now();
		count = run();
		durations.push(performance.now() - t0);
	}
	const ms = median(durations);
	results.push({
		label: c.label,
		bytes,
		lines: content.split('\n').length,
		extracted: count,
		ms: Number(ms.toFixed(2)),
		perSecond: count > 0 ? Math.round(count / (ms / 1000)) : null,
		mbPerSecond: Number((bytes / 1_048_576 / (ms / 1000)).toFixed(1)),
	});
	console.log(`${c.label.padEnd(18)} ${(bytes / 1_048_576).toFixed(2)} MB  ${String(count).padStart(7)}  ${ms.toFixed(2)} ms`);
}
const cpu = cpus()[0]?.model ?? 'unknown CPU';
await Bun.write(
	'benchmark-results.json',
	`${JSON.stringify({ host: `${cpu}, ${Math.round(totalmem() / 1_073_741_824)} GB RAM, Node ${process.versions.node}`, runs: RUNS, results }, null, 2)}\n`,
);
console.log('\nwrote benchmark-results.json');
