/**
 * Reading files and shaping the report, as the Rust CLI's `scan.rs` does.
 *
 * One report per text file. A binary file, a NUL in its first 8 KiB, is not a
 * candidate: it produces no report and is counted. A text file that could not
 * be read is reported with the reason.
 */
import { readFileSync } from 'node:fs';
import { basename, sep } from 'node:path';
import {
	counts,
	extract,
	type Found,
	type Kind,
	resolveFormat,
} from '../extract';
import { decodeUtf8OrThrow, rustIoMessage, withoutBom } from './ioError';

export interface ScanOptions {
	readonly format: string | null;
	readonly kind: Kind | undefined;
	readonly nowMs: number;
}

export interface FileReport {
	readonly schema: 1;
	readonly file: string;
	readonly format: string;
	readonly ids: readonly Found[];
	readonly diagnostics: readonly {
		readonly severity: 'warning';
		readonly code: 'skipped';
		readonly message: string;
	}[];
	readonly summary: { readonly ids: number; readonly refused: number };
}

/** A report, or null for a binary file. */
export function scanFile(
	path: string,
	options: ScanOptions,
): FileReport | null {
	const file = sep === '/' ? path : path.split(sep).join('/');
	const format = options.format ?? resolveFormat(undefined, basename(path));
	let bytes: Buffer;
	try {
		bytes = readFileSync(path);
	} catch (error) {
		return skipped(file, format, rustIoMessage(error));
	}
	if (bytes.subarray(0, 8192).includes(0)) return null;
	let content: string;
	try {
		content = decodeUtf8OrThrow(bytes);
	} catch {
		return skipped(file, format, 'not UTF-8 text');
	}
	return scanContent(withoutBom(content), file, format, options);
}

export function scanContent(
	content: string,
	file: string,
	format: string,
	options: ScanOptions,
): FileReport {
	const ids = extract(content, format as never, {
		clock: { nowMs: options.nowMs },
		kind: options.kind,
	});
	const [named, refused] = counts(ids);
	return Object.freeze({
		schema: 1,
		file,
		format,
		ids,
		diagnostics: [],
		summary: { ids: named, refused },
	});
}

function skipped(file: string, format: string, message: string): FileReport {
	return Object.freeze({
		schema: 1,
		file,
		format,
		ids: [],
		diagnostics: [
			{ severity: 'warning' as const, code: 'skipped' as const, message },
		],
		summary: { ids: 0, refused: 0 },
	});
}

/** grep's convention, with a refusal or an unread file failing only `--strict`. */
export function exitCode(
	reports: readonly FileReport[],
	strict: boolean,
): number {
	const unsure = reports.some(
		(report) => report.diagnostics.length > 0 || report.summary.refused > 0,
	);
	if (strict && unsure) return 2;
	return reports.some((report) => report.summary.ids > 0) ? 0 : 1;
}

export function describe(report: FileReport, found: Found): string {
	const where = `${report.file}:${found.line}:${found.column}`;
	if (found.refused === undefined) {
		const kind = found.kind ?? 'id';
		const version = found.version === undefined ? '' : ` v${found.version}`;
		const when = found.timestamp === undefined ? '' : `  ${found.timestamp}`;
		return `${where}  ${kind}${version}  ${found.value}${when}`;
	}
	return `${where}  refused (${found.refused})  ${found.value}  — ${found.detail ?? 'no reason given'}`;
}
