/**
 * Requires the npm command line to behave exactly as the Rust one does.
 *
 * Both install a command named `ids-le`, so a person or a CI step cannot tell
 * which one they have. This is a different contract from
 * `check-extraction-parity.ts`, which holds the extension's engine equal to
 * the crate's: here the whole terminal surface is compared, the walk, the
 * identifiers and refusals, the key paths, the exit codes, stdout and stderr
 * byte for byte, and every reply `ids-le mcp` writes, byte for byte too.
 *
 * Every case runs both binaries over one generated tree with stdout piped,
 * which is how a script sees them. `--version` is compared too: every surface
 * carries one version, and two binaries of one name disagreeing about it is
 * the first thing a person comparing them would see.
 *
 * Run: bun run build:cli && cargo build --locked --manifest-path crate/Cargo.toml
 *      bun scripts/check-cli-parity.ts
 */
import { spawnSync } from 'node:child_process';
import {
	chmodSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	rmdirSync,
	symlinkSync,
	unlinkSync,
	writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const ROOT = join(import.meta.dir, '..');
const RUST =
	process.env.IDS_LE_BIN ??
	join(
		ROOT,
		'crate',
		'target',
		'debug',
		`ids-le${process.platform === 'win32' ? '.exe' : ''}`,
	);
const NODE_CLI = join(ROOT, 'cli', 'cli.js');
/**
 * Node, by name, not `process.execPath`: this script runs under Bun, and the
 * package is run by Node wherever `npx` runs it. Bun's errors are not Node's.
 */
const NODE = process.env.NODE_BIN ?? 'node';

interface Case {
	readonly args: readonly string[];
	readonly stdin?: string | Buffer;
}

interface Outcome {
	readonly status: number | null;
	readonly stdout: string;
	readonly stderr: string;
}

for (const [label, path] of [
	['the Rust binary', RUST],
	['cli/cli.js', NODE_CLI],
] as const) {
	if (!existsSync(path)) {
		console.error(`FAIL: ${label} is missing at ${path}. Build it first.`);
		process.exit(1);
	}
}

const tree = mkdtempSync(join(tmpdir(), 'ids-le-cli-parity-'));
const created: string[] = [];
const dirs: string[] = [tree];

function dir(relative: string): void {
	mkdirSync(join(tree, relative));
	dirs.push(join(tree, relative));
}

function file(relative: string, content: string | Buffer): void {
	writeFileSync(join(tree, relative), content);
	created.push(join(tree, relative));
}

function link(relative: string, target: string): void {
	// Windows grants symlinks to administrators and developer mode only, and
	// both binaries then see the same tree without the link.
	try {
		symlinkSync(target, join(tree, relative));
	} catch {
		return;
	}
	created.push(join(tree, relative));
}

const canLock = process.platform !== 'win32' && process.getuid?.() !== 0;

const IDS = [
	'uuid v4 f47ac10b-58cc-4372-a567-0e02b2c3d479',
	'uuid v7 018f3e5c-8a9b-7cde-8f01-23456789abcd and v1 6ba7b810-9dad-11d1-80b4-00c04fd430c8',
	'ulid 01ARZ3NDEKTSV4RRFFQ69G5FAV, objectid 507f1f77bcf86cd799439011',
	'snowflake 1541815603606036480 and nanoid V1StGXR8_Z5jdHi6B-myT',
	'nil 00000000-0000-0000-0000-000000000000, max ffffffff-ffff-ffff-ffff-ffffffffffff',
	'bad variant f47ac10b-58cc-4372-c567-0e02b2c3d479, old objectid 000000000000000000000000',
	'ambiguous 0123456789abcdef01234567 and ünïcödé f47ac10b-58cc-4372-a567-0e02b2c3d479',
	'',
].join('\n');

function build(): void {
	dir('repo');
	dir('repo/.git');
	file('repo/.gitignore', 'ignored.txt\nbuild/\n');
	file('repo/.ignore', 'dot-ignored.txt\n');
	file('repo/ids.txt', IDS);
	file('repo/config.json', '{"service": {"id": "f47ac10b-58cc-4372-a567-0e02b2c3d479", "list": ["01ARZ3NDEKTSV4RRFFQ69G5FAV"]}}\n');
	file('repo/config.yaml', 'service:\n  id: 507f1f77bcf86cd799439011\n  ids:\n    - 1541815603606036480\n');
	file('repo/rows.csv', 'name,id\nalice,f47ac10b-58cc-4372-a567-0e02b2c3d479\n');
	file('repo/rows.tsv', 'name\tid\nbob\t01ARZ3NDEKTSV4RRFFQ69G5FAV\n');
	file('repo/settings.toml', '[db]\nid = "507f1f77bcf86cd799439011"\n');
	file('repo/setup.ini', '[a]\nid = 6ba7b810-9dad-11d1-80b4-00c04fd430c8\n');
	file('repo/.env', 'REQUEST_ID=f47ac10b-58cc-4372-a567-0e02b2c3d479\n');
	file('repo/app.env', 'ID=01ARZ3NDEKTSV4RRFFQ69G5FAV\n');
	file('repo/bom.json', '\uFEFF{"id": "f47ac10b-58cc-4372-a567-0e02b2c3d479"}\n');
	file('repo/crlf.txt', 'f47ac10b-58cc-4372-a567-0e02b2c3d479\r\nnext\r\n');
	file('repo/clean.md', 'no identifiers here\n');
	file('repo/empty.txt', '');
	file('repo/latin1.txt', Buffer.from([0x63, 0x61, 0x66, 0xe9, 0x0a]));
	file('repo/binary.dat', Buffer.from([0x00, 0x01, 0x02, 0x03]));
	// ripgrep's sniff reads 8 KiB: a NUL inside it is binary, one past it is not.
	file('repo/late-nul.dat', Buffer.concat([Buffer.alloc(100, 0x61), Buffer.from([0])]));
	file('repo/past-sniff.txt', Buffer.concat([Buffer.from('f47ac10b-58cc-4372-a567-0e02b2c3d479 '), Buffer.alloc(8192, 0x61), Buffer.from([0])]));
	file('repo/ünïcödé.txt', 'f47ac10b-58cc-4372-a567-0e02b2c3d479\n');
	file('repo/ignored.txt', 'f47ac10b-58cc-4372-a567-0e02b2c3d479\n');
	file('repo/dot-ignored.txt', 'f47ac10b-58cc-4372-a567-0e02b2c3d479\n');
	file('repo/.hidden.txt', 'f47ac10b-58cc-4372-a567-0e02b2c3d479\n');
	dir('repo/build');
	file('repo/build/out.txt', 'f47ac10b-58cc-4372-a567-0e02b2c3d479\n');
	dir('repo/nested');
	file('repo/nested/.gitignore', 'local.txt\n');
	file('repo/nested/local.txt', 'f47ac10b-58cc-4372-a567-0e02b2c3d479\n');
	file('repo/nested/kept.txt', '01ARZ3NDEKTSV4RRFFQ69G5FAV\n');
	link('repo/linked.txt', 'ids.txt');
	link('repo/linked-dir', 'nested');
	dir('none');
	file('none/plain.txt', 'nothing to see\n');
	dir('refusals');
	file('refusals/only.txt', '00000000-0000-0000-0000-000000000000\n');
	dir('many');
	file('many/big.txt', `${IDS}`.repeat(800));
	if (canLock) {
		file('repo/locked.txt', 'x\n');
		chmodSync(join(tree, 'repo', 'locked.txt'), 0o000);
		dir('closed');
		dir('closed/shut');
		file('closed/shut/inside.txt', 'x\n');
		file('closed/open.txt', 'f47ac10b-58cc-4372-a567-0e02b2c3d479\n');
		chmodSync(join(tree, 'closed', 'shut'), 0o000);
	}
}

function cases(): readonly Case[] {
	const repo = join(tree, 'repo');
	const at = (name: string) => join(repo, name);
	const corpus = join(ROOT, 'crate', 'fixtures', 'documents');
	return [
		{ args: ['--help'] },
		{ args: ['-h'] },
		{ args: ['--version'] },
		{ args: ['-V'] },
		{ args: [repo, '--help'] },
		{ args: [] },
		{ args: [repo] },
		{ args: [`${repo}/`] },
		{ args: ['repo'] },
		{ args: ['./repo/nested/'] },
		{ args: [repo, '--hidden'] },
		{ args: [repo, '--no-ignore'] },
		{ args: [repo, '--hidden', '--no-ignore'] },
		{ args: [repo, '--strict'] },
		{ args: [repo, '--kind', 'uuid'] },
		{ args: [repo, '--kind', ' ULID '] },
		{ args: [repo, '--kind', 'uuid\u0085'] },
		{ args: [repo, '--kind', 'uuid\uFEFF'] },
		{ args: [repo, '--kind', 'guid'] },
		{ args: [repo, '--kind'] },
		{ args: [repo, '--format', 'json'] },
		{ args: [repo, '--format', '.YAML'] },
		{ args: [repo, '--format', 'handwriting'] },
		{ args: [repo, '--format'] },
		{ args: [repo, '--nope'] },
		{ args: [repo, '-'] },
		{ args: [at('ids.txt')] },
		{ args: [at('config.json'), at('config.yaml'), at('rows.csv'), at('rows.tsv')] },
		{ args: [at('settings.toml'), at('setup.ini'), at('.env'), at('app.env')] },
		{ args: [at('bom.json'), at('crlf.txt'), at('clean.md'), at('empty.txt')] },
		{ args: [at('latin1.txt'), at('binary.dat')] },
		{ args: [at('latin1.txt'), '--strict'] },
		{ args: [at('binary.dat'), '--strict'] },
		{ args: [at('ünïcödé.txt')] },
		{ args: [at('ignored.txt'), at('.hidden.txt'), at('dot-ignored.txt')] },
		{ args: [at('ids.txt'), at('ids.txt')] },
		{ args: [at('nested'), at('nested/kept.txt')] },
		{ args: [at('linked.txt')] },
		{ args: [at('linked-dir')] },
		{ args: [join(tree, 'none')] },
		{ args: [join(tree, 'refusals')] },
		{ args: [join(tree, 'refusals'), '--strict'] },
		{ args: [join(tree, 'many')] },
		...(canLock
			? [
					{ args: [join(tree, 'closed')] },
					{ args: [at('locked.txt')] },
					{ args: [at('locked.txt'), '--strict'] },
				]
			: []),
		{ args: [join(tree, 'missing')] },
		{ args: [repo, join(tree, 'missing')] },
		{ args: [join(tree, 'mïssing')] },
		{ args: ['--stdin'] },
		{ args: ['--stdin', repo] },
		{ args: ['--stdin'], stdin: IDS },
		{ args: ['--stdin', '--kind', 'snowflake'], stdin: IDS },
		{ args: ['--stdin', '--format', 'json'], stdin: '{"a": {"b": "f47ac10b-58cc-4372-a567-0e02b2c3d479"}}' },
		{ args: ['--stdin'], stdin: `\uFEFF${IDS}` },
		{ args: ['--stdin'], stdin: Buffer.from([0x63, 0xe9, 0x0a]) },
		{ args: ['--stdin'], stdin: Buffer.from([0x61, 0x00, 0x62]) },
		{ args: ['--stdin'], stdin: '' },
		{ args: [corpus] },
		{ args: [corpus, '--hidden', '--no-ignore', '--strict'] },
	];
}

function run(command: readonly string[], testCase: Case): Outcome {
	const [program, ...rest] = command as [string, ...string[]];
	const result = spawnSync(program, [...rest, ...testCase.args], {
		input: testCase.stdin ?? '',
		cwd: tree,
		maxBuffer: 256 * 1024 * 1024,
	});
	return {
		status: result.status,
		stdout: result.stdout.toString('utf8'),
		stderr: result.stderr.toString('utf8'),
	};
}

function compareCli(): string[] {
	const failures: string[] = [];
	for (const testCase of cases()) {
		const rust = run([RUST], testCase);
		const node = run([NODE, NODE_CLI], testCase);
		for (const field of ['status', 'stdout', 'stderr'] as const) {
			if (rust[field] === node[field]) continue;
			failures.push(
				`ids-le ${testCase.args.join(' ')}\n  ${field} differs\n` +
					describeDifference(String(rust[field]), String(node[field])),
			);
		}
	}
	return failures;
}

function mcpLines(): readonly string[] {
	const repo = join(tree, 'repo');
	let id = 0;
	const request = (method: string, params?: unknown) =>
		JSON.stringify({
			jsonrpc: '2.0',
			id: ++id,
			method,
			...(params === undefined ? {} : { params }),
		});
	const call = (name: string, args?: unknown) =>
		request('tools/call', args === undefined ? { name } : { name, arguments: args });
	const extract = (args: Record<string, unknown>) => call('extract_ids', args);
	const scan = (args: Record<string, unknown>) => call('ids_le_scan', args);
	return [
		request('initialize', { protocolVersion: '2024-11-05' }),
		JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }),
		request('tools/list'),
		request('ping'),
		request('resources/list'),
		request('tools/call'),
		request('tools/call', { arguments: {} }),
		request('tools/call', null),
		call('no_such_tool', {}),
		JSON.stringify({ jsonrpc: '2.0', id: 'tëxt-id', method: 'ping' }),
		JSON.stringify({ jsonrpc: '2.0', id: null, method: 'ping' }),
		JSON.stringify({ id: 77, method: 'ping' }),
		JSON.stringify({ jsonrpc: '2.0', id: 78 }),
		'not json',
		'',
		'   ',
		'[1, 2]',
		`${request('ping')}\r`,
		extract({ content: IDS }),
		extract({ content: IDS, kind: 'uuid', maxResults: 2 }),
		extract({ content: IDS, kind: ' Snowflake ' }),
		extract({ content: IDS, kind: 'guid' }),
		extract({ content: IDS, kind: 7 }),
		extract({ content: '{"a": "507f1f77bcf86cd799439011"}', format: 'json' }),
		extract({ content: 7 }),
		extract({}),
		call('extract_ids', null),
		scan({ path: repo }),
		scan({ paths: [repo, join(tree, 'none')], kind: 'ulid' }),
		scan({ path: repo, hidden: true, ignored: true }),
		scan({ path: repo, format: 'json' }),
		scan({ path: repo, format: 7 }),
		scan({ path: repo, kind: 'guid' }),
		scan({ path: repo, kind: 7 }),
		scan({ path: join(repo, 'ids.txt'), hidden: 'yes' }),
		scan({ path: join(tree, 'refusals') }),
		scan({ paths: [] }),
		scan({ paths: [3] }),
		scan({ path: 3, paths: [join(repo, 'clean.md')] }),
		scan({}),
		scan({ path: join(tree, 'missing') }),
		...(canLock ? [scan({ path: join(tree, 'closed') })] : []),
	];
}

function serve(command: readonly string[], input: string | Buffer): Outcome {
	const [program, ...rest] = command as [string, ...string[]];
	const result = spawnSync(program, [...rest, 'mcp'], { input, cwd: tree });
	return {
		status: result.status,
		stdout: result.stdout.toString('utf8'),
		stderr: result.stderr.toString('utf8'),
	};
}

function compareMcp(): string[] {
	const failures: string[] = [];
	const lines = mcpLines();
	const sessions: readonly (readonly [string, string | Buffer])[] = [
		['the whole session', `${lines.join('\n')}\n`],
		['a last line with no newline', lines[3] as string],
		[
			'a line that is not UTF-8',
			Buffer.concat([
				Buffer.from(`${lines[3]}\n`),
				Buffer.from([0x7b, 0xff, 0x7d, 0x0a]),
				Buffer.from(`${lines[3]}\n`),
			]),
		],
	];
	for (const [label, input] of sessions) {
		const rust = serve([RUST], input);
		const node = serve([NODE, NODE_CLI], input);
		for (const field of ['status', 'stdout', 'stderr'] as const) {
			if (rust[field] === node[field]) continue;
			failures.push(
				`ids-le mcp, ${label}\n  ${field} differs\n` +
					describeDifference(String(rust[field]), String(node[field])),
			);
		}
	}
	return failures;
}

/** The first differing line, which is the one worth reading in a long output. */
function describeDifference(rust: string, node: string): string {
	const left = rust.split('\n');
	const right = node.split('\n');
	const index = left.findIndex((line, at) => line !== right[at]);
	const at = index === -1 ? left.length : index;
	const clip = (line: string | undefined) =>
		line === undefined || line.length <= 2000 ? line : `${line.slice(0, 2000)}…`;
	return `  line ${at + 1}\n  rust: ${clip(left[at])}\n  node: ${clip(right[at])}`;
}

function cleanUp(): void {
	const unlock: readonly (readonly [string, number])[] = [
		[join(tree, 'repo', 'locked.txt'), 0o600],
		[join(tree, 'closed', 'shut'), 0o700],
	];
	for (const [path, mode] of unlock) if (existsSync(path)) chmodSync(path, mode);
	for (const path of created) unlinkSync(path);
	for (const path of [...dirs].reverse()) rmdirSync(path);
}

build();
let failures: string[];
try {
	failures = [...compareCli(), ...compareMcp()];
} finally {
	cleanUp();
}

if (failures.length > 0) {
	console.error(failures.join('\n\n'));
	console.error(`\nFAIL: ${failures.length} difference(s) between the npm and Rust CLIs`);
	process.exit(1);
}
console.log(
	`OK: the npm CLI matches the Rust CLI on ${cases().length} commands and ${mcpLines().length} MCP lines`,
);
