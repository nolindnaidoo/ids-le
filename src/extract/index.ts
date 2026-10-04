import { candidates } from './candidate';
import { CLASSIFIERS } from './kinds';
import { keyAt, keySpans, type Reader } from './locate';
import {
	type Clock,
	type Kind,
	type Reason,
	route,
	type Variant,
} from './policy';

export { FALLBACK_FORMAT, resolveFormat, SUPPORTED_FORMATS } from './format';
export { type Clock, KIND_NAMES, type Kind } from './policy';

/**
 * Every identifier and every refusal in a document, in document order — the
 * crate's `extract::extract`.
 *
 * Field for field the crate's `Found`: `kind` is present and `null` where
 * naming one is what was refused, and the fields that do not apply are absent.
 */
export interface Found {
	readonly kind: Kind | null;
	readonly value: string;
	readonly line: number;
	/** 1-based, in UTF-16 code units, as an editor counts. */
	readonly column: number;
	readonly key?: string;
	readonly valid: boolean;
	readonly version?: number;
	readonly variant?: Variant;
	readonly timestamp?: string;
	readonly refused?: Reason;
	readonly detail?: string;
}

export interface Options {
	readonly clock: Clock;
	/** A view over which identifiers are named: every refusal survives it. */
	readonly kind: Kind | undefined;
}

export function extract(
	text: string,
	reader: Reader,
	options: Options,
): Found[] {
	const spans = keySpans(text, reader);
	const lineStarts = [0];
	for (let at = text.indexOf('\n'); at !== -1; at = text.indexOf('\n', at + 1))
		lineStarts.push(at + 1);

	const rows: Found[] = [];
	for (const candidate of candidates(text)) {
		// An empty path is the JSON root or a headerless CSV: no key at all.
		const path = keyAt(spans, candidate.start);
		const key = path === '' ? undefined : path;
		const verdict = route(CLASSIFIERS, candidate.text, key, options.clock);
		if (verdict.verdict === 'ignored') continue;

		const line = lineOf(lineStarts, candidate.start);
		const located = {
			value: candidate.text,
			line: line + 1,
			column: candidate.start - (lineStarts[line] as number) + 1,
			...(key === undefined ? {} : { key }),
		};
		const decoded = {
			...(verdict.version === undefined ? {} : { version: verdict.version }),
			...(verdict.variant === undefined ? {} : { variant: verdict.variant }),
			...(verdict.timestamp === undefined
				? {}
				: { timestamp: verdict.timestamp }),
		};
		if (verdict.verdict === 'named') {
			if (options.kind !== undefined && verdict.kind !== options.kind) continue;
			rows.push({ kind: verdict.kind, ...located, valid: true, ...decoded });
			continue;
		}
		rows.push({
			kind: verdict.kind,
			...located,
			valid: false,
			...decoded,
			refused: verdict.reason,
			detail: verdict.detail,
		});
	}
	return rows;
}

function lineOf(lineStarts: readonly number[], offset: number): number {
	let low = 0;
	let high = lineStarts.length;
	while (low < high) {
		const mid = (low + high) >>> 1;
		if ((lineStarts[mid] as number) <= offset) low = mid + 1;
		else high = mid;
	}
	return low - 1;
}

/** Named rows and refused rows, counted from the rows themselves. */
export function counts(rows: readonly Found[]): [number, number] {
	const named = rows.filter((row) => row.valid).length;
	return [named, rows.length - named];
}
