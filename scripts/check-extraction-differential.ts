/**
 * `extract_ids` is offered by BOTH servers — the npm one in `src/mcp/tools.ts`
 * and the Rust one in `crate/src/mcp/extract.rs`. One tool name, one schema,
 * two implementations, so the contract is identical output.
 *
 * `crate/fixtures/mcp-extract-ids.json` pins the cases somebody thought of.
 * This generates them: documents in every format, carrying every kind of
 * identifier valid, malformed and ambiguous, under keys that do and do not
 * name an id — because the key path is evidence for four of the five kinds,
 * and a reader that addressed a value differently would change a verdict.
 *
 * Both servers read their own clock. Generated timestamps never sit within
 * seconds of the plausibility window's edges, so the two clocks cannot
 * disagree about a verdict.
 *
 * Run: bun scripts/check-extraction-differential.ts
 *   IDS_LE_DIFFERENTIAL_SEED=<n>  reproduce a specific failure
 *   IDS_LE_DIFFERENTIAL_CASES=<n> how many documents (default 1500)
 *   IDS_LE_BIN=<path>             the Rust binary (default the release build)
 */
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { TOOLS } from '../src/mcp/tools';

const ROOT = join(import.meta.dir, '..');
const BINARY = process.env.IDS_LE_BIN ?? join(ROOT, 'crate', 'target', 'release', 'ids-le');
const SEED = Number(process.env.IDS_LE_DIFFERENTIAL_SEED ?? 20261003);
const CASES = Number(process.env.IDS_LE_DIFFERENTIAL_CASES ?? 1500);

function seeded(seed: number): () => number {
	let state = seed >>> 0;
	return () => {
		state = (state + 0x6d2b79f5) >>> 0;
		let t = state;
		t = Math.imul(t ^ (t >>> 15), t | 1);
		t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
}

const HEX = '0123456789abcdef';
const CROCKFORD = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
const BASE64URL = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';

function identifiers(random: () => number): string {
	const pick = <T>(list: readonly T[]): T => list[Math.floor(random() * list.length)] as T;
	const chars = (alphabet: string, n: number) => Array.from({ length: n }, () => pick([...alphabet])).join('');
	const roll = random();
	if (roll < 0.25) {
		const hex = chars(HEX, 32).split('');
		hex[12] = pick([...'1234567890abcdef']);
		hex[16] = pick([...'89ab89ab01cdef']);
		if (random() < 0.15) for (let i = 13; i < 28; i++) hex[i] = '0';
		if (random() < 0.1) hex[3] = pick(['g', 'z', '_']);
		const h = hex.join('');
		const out = `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
		return random() < 0.2 ? out.toUpperCase() : out;
	}
	if (roll < 0.4) {
		const ulid = pick(['0', '1', '7', '8', 'Z']) + chars(CROCKFORD, 25);
		const r = random();
		if (r < 0.2) return ulid.toLowerCase();
		if (r < 0.3) return ulid.slice(0, 10) + ulid.slice(10).toLowerCase();
		if (r < 0.4) return `${ulid.slice(0, 5)}U${ulid.slice(6)}`;
		return ulid;
	}
	if (roll < 0.55) return chars(random() < 0.5 ? BASE64URL : BASE64URL.slice(0, 62), 21);
	if (roll < 0.7) return chars(HEX, pick([23, 24, 24, 24, 25]));
	if (roll < 0.82) return `${pick(['1', '9', '4'])}${chars('0123456789', pick([16, 17, 18]))}`;
	if (roll < 0.9) return chars(HEX, pick([31, 32, 33]));
	return pick([
		'00000000-0000-0000-0000-000000000000',
		'ffffffff-ffff-ffff-ffff-ffffffffffff',
		'c232ab00-9414-11ec-b3c8-9e6bdeced846',
		'1ec9414c-232a-6b00-b3c8-9e6bdeced846',
		'7fffffff-ffff-7abc-8def-0123456789ab',
		'user_550e8400-e29b-41d4-a716-446655440000',
		'abcdefghijklmnopqrstuvwxyz',
		'SOME_CONSTANT_NAME_ER',
	]);
}

const KEYS = [
	'id', 'userId', 'user_id', 'USER-ID', '_id', '$oid', 'objectId', 'checksum', 'digest', 'session.ulid', 'nanoId',
	'snowflake', 'discord.channel_id', 'guild_id', 'tweet_id', 'status_id', 'commitId', 'name', 'ulids', 'note', 'café',
];

type Builder = (pairs: ReadonlyArray<readonly [string, string]>, random: () => number) => string;

const FORMATS: ReadonlyArray<readonly [string, Builder]> = [
	['json', (p, r) => `{\n${p.map(([k, v], i) => (i % 3 === 2 && r() < 0.5 ? `  "list": ["${v}", {"${k}": "${v}"}]` : `  "${k}": ${/^\d+$/.test(v) && r() < 0.5 ? v : `"${v}"`}`)).join(',\n')}${r() < 0.2 ? ',' : ''}\n  // trailing ${p[0]?.[1] ?? ''}\n}\n`],
	['yaml', (p, r) => `${p.map(([k, v], i) => (i % 2 ? `items:\n  - ${k}: ${v}\n    name: ${v}  # ${v}` : `${k}: ${r() < 0.3 ? `"${v}"` : v}`)).join('\n')}\n　　nested: ${p[0]?.[1] ?? ''}\n`],
	['toml', (p) => `[server]\n${p.map(([k, v]) => `${k.replace(/[.$]/g, '_')} = "${v}" # ${v}`).join('\n')}\n[[items]]\nid = ${p[0]?.[1] ?? '1'}\n`],
	['ini', (p) => `; ${p[0]?.[1] ?? ''}\n[section]\n${p.map(([k, v], i) => (i % 2 ? `${k}: ${v}` : `${k} = ${v} ; note ${v}`)).join('\n')}\n`],
	['env', (p) => `${p.map(([k, v], i) => `${i % 2 ? 'export ' : ''}${k.replace(/[^A-Za-z0-9_]/g, '_').toUpperCase()}=${i % 3 ? `"${v} # x"` : `${v} # ${v}`}`).join('\n')}\n`],
	['csv', (p) => `${p.map(([k]) => k).join(',')}\n${p.map(([, v], i) => (i % 2 ? `"${v}"` : v)).join(',')}\n${p.map(([, v]) => v).reverse().join(',')}\n`],
	['tsv', (p) => `${p.map(([k]) => k).join('\t')}\n${p.map(([, v]) => v).join('\t')}\n`],
	['text', (p) => p.map(([k, v]) => `The ${k} is ${v}, see (${v}).`).join('\n')],
];

interface Generated {
	readonly name: string;
	readonly args: Record<string, unknown>;
}

function generate(count: number, seed: number): Generated[] {
	const random = seeded(seed);
	const pick = <T>(list: readonly T[]): T => list[Math.floor(random() * list.length)] as T;
	const out: Generated[] = [];
	for (let index = 0; index < count; index++) {
		const [format, build] = pick(FORMATS);
		const pairs = Array.from({ length: 1 + Math.floor(random() * 6) }, () => [pick(KEYS), identifiers(random)] as const);
		let content = build(pairs, random);
		if (random() < 0.1) content = content.replace(/\n/g, '\r\n');
		if (random() < 0.1) content = `é😀 ${content}`;
		const args: Record<string, unknown> = { content };
		const addressing = random();
		if (addressing < 0.5) args.format = format;
		else if (addressing < 0.75) args.filename = pick([`config.${format}`, '.env.local', 'notes.md', 'x.conf']);
		if (random() < 0.12) args.kind = pick(['uuid', 'ulid', 'nanoid', 'objectid', 'snowflake']);
		if (random() < 0.08) args.maxResults = 1 + Math.floor(random() * 3);
		out.push({ name: `${index}:${format}`, args });
	}
	return out;
}

function canonical(value: unknown): string {
	if (value === null || typeof value !== 'object') return JSON.stringify(value);
	if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
	const entries = Object.entries(value as Record<string, unknown>)
		.filter(([, item]) => item !== undefined)
		.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
	return `{${entries.map(([k, v]) => `${JSON.stringify(k)}:${canonical(v)}`).join(',')}}`;
}

async function fromNpm(documents: readonly Generated[]): Promise<string[]> {
	const tool = TOOLS.find((candidate) => candidate.name === 'extract_ids');
	if (!tool) throw new Error('the npm server no longer offers extract_ids');
	const answers: string[] = [];
	for (const document of documents) {
		try {
			answers.push(canonical(JSON.parse(JSON.stringify(await tool.handler(document.args)))));
		} catch (error) {
			answers.push(`error: ${error instanceof Error ? error.message : String(error)}`);
		}
	}
	return answers;
}

async function fromCrate(documents: readonly Generated[]): Promise<string[]> {
	if (!existsSync(BINARY)) throw new Error(`no binary at ${BINARY} — build it first: cd crate && cargo build --release`);
	const child = Bun.spawn([BINARY, 'mcp'], { stdin: 'pipe', stdout: 'pipe', stderr: 'pipe' });
	const draining = new Response(child.stdout).text();
	child.stdin.write(
		`${documents
			.map((document, id) =>
				JSON.stringify({ jsonrpc: '2.0', id, method: 'tools/call', params: { name: 'extract_ids', arguments: document.args } }),
			)
			.join('\n')}\n`,
	);
	child.stdin.end();
	const stdout = await draining;
	await child.exited;
	const answers: string[] = new Array(documents.length);
	for (const line of stdout.split('\n')) {
		if (line.trim().length === 0) continue;
		const response = JSON.parse(line) as {
			id: number;
			result?: { structuredContent?: unknown; isError?: boolean; content?: { text: string }[] };
			error?: unknown;
		};
		if (response.error !== undefined) throw new Error(`the crate server refused document ${response.id}: ${JSON.stringify(response.error)}`);
		answers[response.id] = response.result?.isError
			? `error: ${response.result.content?.[0]?.text}`
			: canonical(response.result?.structuredContent);
	}
	const missing = answers.findIndex((answer) => answer === undefined);
	if (missing !== -1) throw new Error(`the crate server never answered document ${missing}: ${await new Response(child.stderr).text()}`);
	return answers;
}

const documents = generate(CASES, SEED);
console.log(`differential: ${documents.length} generated documents, seed ${SEED}, binary ${BINARY.replace(ROOT, '.')}`);
const [npm, crate] = await Promise.all([fromNpm(documents), fromCrate(documents)]);
const failures: string[] = [];
let named = 0;
let refused = 0;
for (const [index, document] of documents.entries()) {
	const ours = npm[index] as string;
	if (!ours.startsWith('error:')) {
		const data = JSON.parse(ours).data;
		named += data.named;
		refused += data.refused;
	}
	if (ours !== crate[index]) {
		failures.push(
			`the two extract_ids servers disagree on "${document.name}"\n  arguments: ${JSON.stringify(document.args).slice(0, 700)}\n  npm:   ${ours.slice(0, 900)}\n  crate: ${(crate[index] as string).slice(0, 900)}`,
		);
	}
}
console.log(`  ${named} identifiers named, ${refused} refused, across every format`);
if (failures.length > 0) {
	console.error(`\nDIFFERENTIAL FAILED — ${failures.length} problem(s):\n`);
	for (const failure of failures.slice(0, 8)) console.error(`${failure}\n`);
	process.exit(1);
}
console.log('OK: both extract_ids servers gave identical answers on every document.');
