/**
 * The terminal surface, as the Rust CLI's `cli.rs` runs it.
 *
 * stdout is protocol: one JSON report per line whenever something is reading
 * it, and nothing when it is a terminal, because the stderr listing already
 * shows every row a person reads there.
 */
import { FALLBACK_FORMAT } from '../extract/format';
import { type Cli, parse } from './args';
import { decodeUtf8OrThrow, rustIoMessage, withoutBom } from './ioError';
import {
	describe,
	exitCode,
	type FileReport,
	scanContent,
	scanFile,
} from './scan';
import { USAGE } from './usage';
import { collect } from './walk';

export interface RunIo {
	readonly stdout: (text: string) => void;
	readonly stderr: (text: string) => void;
	readonly stdoutIsTerminal: boolean;
	readonly readStdin: () => Promise<Uint8Array>;
	readonly version: string;
	readonly nowMs: number;
}

export async function run(args: readonly string[], io: RunIo): Promise<number> {
	const first = args[0];
	if (first === '--help' || first === '-h') {
		io.stdout(`${USAGE}\n`);
		return 0;
	}
	if (first === '--version' || first === '-V') {
		io.stdout(`ids-le ${io.version}\n`);
		return 0;
	}
	try {
		return await execute(parse(args), io);
	} catch (error) {
		io.stderr(
			`ids-le: ${error instanceof Error ? error.message : String(error)}\n`,
		);
		return 2;
	}
}

async function execute(cli: Cli, io: RunIo): Promise<number> {
	const options = { format: cli.format, kind: cli.kind, nowMs: io.nowMs };
	let reports: FileReport[];
	let binary = 0;
	if (cli.stdin) {
		let content: string;
		try {
			content = decodeUtf8OrThrow(await io.readStdin());
		} catch (error) {
			throw new Error(`could not read stdin: ${rustIoMessage(error)}`);
		}
		reports = [
			scanContent(
				withoutBom(content),
				'<stdin>',
				cli.format ?? FALLBACK_FORMAT,
				options,
			),
		];
	} else {
		const files = collect(cli.inputs, cli.walk);
		reports = [];
		for (const file of files) {
			const scanned = scanFile(file, options);
			if (scanned) reports.push(scanned);
		}
		binary = files.length - reports.length;
	}

	if (!io.stdoutIsTerminal) {
		io.stdout(reports.map((each) => `${JSON.stringify(each)}\n`).join(''));
	}
	io.stderr(summary(reports, binary));
	return exitCode(reports, cli.strict);
}

export function summary(
	reports: readonly FileReport[],
	binary: number,
): string {
	const lines: string[] = [];
	let ids = 0;
	let refused = 0;
	for (const report of reports) {
		for (const diagnostic of report.diagnostics)
			lines.push(`${report.file}: ${diagnostic.message}`);
		for (const found of report.ids) lines.push(describe(report, found));
		ids += report.summary.ids;
		refused += report.summary.refused;
	}
	lines.push(
		`${plural(ids, 'identifier', 'identifiers')} in ${plural(reports.length, 'file', 'files')}`,
	);
	if (refused > 0) lines.push(`${plural(refused, 'run', 'runs')} refused`);
	if (binary > 0) {
		lines.push(`${plural(binary, 'binary file', 'binary files')} skipped`);
	}
	return `${lines.join('\n')}\n`;
}

function plural(count: number, one: string, many: string): string {
	return `${count} ${count === 1 ? one : many}`;
}
