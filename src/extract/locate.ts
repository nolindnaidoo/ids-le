import {
	byteLength,
	isAsciiWhitespace,
	trim,
	trimEnd,
	trimStart,
} from './text';

/**
 * Where a finding sits in the document's own vocabulary — the crate's
 * `locate.rs` and its six readers.
 *
 * Every reader answers one question: which ranges hold a value, and what is
 * the key path of each. Nothing here decides what a finding is, so a reader
 * that is wrong about a key path costs a key path and never a finding — but a
 * key path is evidence for four of the five kinds, so it costs verdicts too,
 * and these are held to the crate's by the corpus and the differential.
 *
 * Offsets here are UTF-16 indices into the document, where the crate's are
 * bytes. Every byte the crate compares is ASCII, so the logic is the same;
 * the one place a width is compared across lines — YAML indentation — measures
 * UTF-8 bytes, as the crate does.
 */

export type Reader =
	| 'json'
	| 'yaml'
	| 'toml'
	| 'ini'
	| 'env'
	| 'csv'
	| 'tsv'
	| 'text';

export interface KeySpan {
	readonly start: number;
	readonly end: number;
	readonly path: string;
}

export function keySpans(text: string, reader: Reader): KeySpan[] {
	switch (reader) {
		case 'json':
			return jsonSpans(text);
		case 'yaml':
			return yamlSpans(text);
		case 'toml':
			return tomlSpans(text);
		case 'ini':
			return iniSpans(text);
		case 'env':
			return dotenvSpans(text);
		case 'csv':
			return csvSpans(text, ',');
		case 'tsv':
			return csvSpans(text, '\t');
		case 'text':
			return [];
	}
}

/** The key path covering an offset, if one does. */
export function keyAt(
	spans: readonly KeySpan[],
	offset: number,
): string | undefined {
	let low = 0;
	let high = spans.length;
	while (low < high) {
		const mid = (low + high) >>> 1;
		if ((spans[mid] as KeySpan).start <= offset) low = mid + 1;
		else high = mid;
	}
	const span = spans[low - 1];
	return span && offset < span.end ? span.path : undefined;
}

/** Every line with the offset it starts at, trailing `\n`/`\r` removed. */
function lines(text: string): Array<[number, string]> {
	const out: Array<[number, string]> = [];
	let offset = 0;
	while (offset < text.length) {
		const newline = text.indexOf('\n', offset);
		const end = newline === -1 ? text.length : newline + 1;
		out.push([offset, text.slice(offset, end).replace(/[\n\r]+$/, '')]);
		offset = end;
	}
	return out;
}

/**
 * How far a value runs on a line that may end in a comment. A comment
 * character opens a comment only outside quotes and after whitespace.
 */
function valueLength(raw: string, comments: string): number {
	let quote: string | undefined;
	let afterSpace = false;
	let at = 0;
	while (at < raw.length) {
		const character = raw.charAt(at);
		at++;
		if (quote === '"' && character === '\\') {
			at++;
			continue;
		}
		if (quote === character) {
			quote = undefined;
			afterSpace = false;
			continue;
		}
		if (quote !== undefined) {
			afterSpace = false;
			continue;
		}
		if (character === '"' || character === "'") {
			quote = character;
			afterSpace = false;
			continue;
		}
		if (afterSpace && comments.includes(character))
			return trimEnd(raw.slice(0, at - 1)).length;
		afterSpace = isAsciiWhitespace(character);
	}
	return trimEnd(raw).length;
}

function join(segments: readonly string[]): string {
	return segments.filter((segment) => segment !== '').join('.');
}

// ---------------------------------------------------------------- JSON

interface Frame {
	array: boolean;
	index: number;
	expectKey: boolean;
}

function isScalar(character: string): boolean {
	return /[0-9A-Za-z\-+._]/.test(character);
}

function jsonSpans(text: string): KeySpan[] {
	const frames: Frame[] = [];
	const path: string[] = [];
	const spans: KeySpan[] = [];
	let at = 0;
	while (at < text.length) {
		const character = text.charAt(at);
		if (character === '{' || character === '[') {
			const array = character === '[';
			frames.push({ array, index: 0, expectKey: !array });
			path.push(array ? '[0]' : '');
			at++;
		} else if (character === '}' || character === ']') {
			frames.pop();
			path.pop();
			at++;
		} else if (character === ':') {
			const frame = frames.at(-1);
			if (frame) frame.expectKey = false;
			at++;
		} else if (character === ',') {
			const frame = frames.at(-1);
			if (frame) {
				frame.expectKey = !frame.array;
				if (frame.array) {
					frame.index++;
					rename(path, `[${frame.index}]`);
				}
			}
			at++;
		} else if (character === '"') {
			const [start, end, next] = readString(text, at);
			at = next;
			if (frames.at(-1)?.expectKey) {
				rename(path, text.slice(start, end));
				continue;
			}
			spans.push({ start, end, path: join(path) });
		} else if (character === '/') {
			at = skipComment(text, at);
		} else if (isScalar(character)) {
			const start = at;
			while (at < text.length && isScalar(text.charAt(at))) at++;
			spans.push({ start, end: at, path: join(path) });
		} else {
			at++;
		}
	}
	return spans;
}

function rename(path: string[], segment: string): void {
	if (path.length > 0) path[path.length - 1] = segment;
}

/** Content start, content end, and the offset past the closing quote. */
function readString(text: string, open: number): [number, number, number] {
	let at = open + 1;
	while (at < text.length) {
		const character = text.charAt(at);
		if (character === '\\') {
			at += 2;
			continue;
		}
		if (character === '"') return [open + 1, at, at + 1];
		at++;
	}
	return [open + 1, text.length, text.length];
}

function skipComment(text: string, at: number): number {
	const next = text.charAt(at + 1);
	if (next === '/') {
		const newline = text.indexOf('\n', at);
		return newline === -1 ? text.length : newline;
	}
	if (next === '*') {
		const close = text.indexOf('*/', at + 2);
		return close === -1 ? text.length : close + 2;
	}
	return at + 1;
}

// ---------------------------------------------------------------- YAML

/**
 * Indentation decides nesting, and the crate measures it in bytes. Positions
 * are UTF-16 units. Both are kept, and only the bytes are ever compared.
 */
function yamlSpans(text: string): KeySpan[] {
	const stack: Array<[number, string]> = [];
	let counters: Array<[number, number]> = [];
	const spans: KeySpan[] = [];
	const pathOf = () => join(stack.map(([, segment]) => segment));

	for (const [offset, line] of lines(text)) {
		const content = trimStart(line);
		if (content === '' || content.startsWith('#') || content.startsWith('---'))
			continue;
		const indentUnits = line.length - content.length;
		const indent = byteLength(line.slice(0, indentUnits));
		for (let i = stack.length - 1; i >= 0; i--)
			if ((stack[i] as [number, string])[0] >= indent) stack.splice(i, 1);
		counters = counters.filter(([at]) => at <= indent);

		let columnUnits = indentUnits;
		let column = indent;
		let body = content;
		if (body.startsWith('-')) {
			let rest = body.slice(1);
			if (rest.startsWith(' ')) rest = rest.slice(1);
			stack.push([indent, `[${nextIndex(counters, indent)}]`]);
			const consumed = body.length - rest.length;
			columnUnits += consumed;
			column += consumed;
			body = rest;
		}

		const split = splitKey(body);
		if (!split) {
			const length = valueLength(body, '#');
			if (length > 0)
				spans.push({
					start: offset + columnUnits,
					end: offset + columnUnits + length,
					path: pathOf(),
				});
			continue;
		}
		const [key, valueAt] = split;
		stack.push([column, key]);
		const value = body.slice(valueAt);
		const leading = value.length - trimStart(value).length;
		const length = valueLength(value, '#');
		if (length <= leading) continue;
		spans.push({
			start: offset + columnUnits + valueAt + leading,
			end: offset + columnUnits + valueAt + length,
			path: pathOf(),
		});
	}
	return spans;
}

function nextIndex(counters: Array<[number, number]>, indent: number): number {
	const found = counters.find(([at]) => at === indent);
	if (found) {
		found[1]++;
		return found[1] - 1;
	}
	counters.push([indent, 1]);
	return 0;
}

/** `key: value`, where the colon is followed by a space or ends the line. */
function splitKey(body: string): [string, number] | undefined {
	for (let at = 0; at < body.length; at++) {
		if (body.charAt(at) !== ':') continue;
		if (at + 1 === body.length || body.charAt(at + 1) === ' ') {
			const key = trim(body.slice(0, at)).replace(/^["']+|["']+$/g, '');
			return key === '' ? undefined : [key, at + 1];
		}
	}
	return undefined;
}

// ---------------------------------------------------------------- TOML

function tomlSpans(text: string): KeySpan[] {
	let table = '';
	const spans: KeySpan[] = [];
	for (const [offset, line] of lines(text)) {
		const trimmed = trim(line);
		if (trimmed === '' || trimmed.startsWith('#')) continue;
		const name = tomlHeader(trimmed);
		if (name !== undefined) {
			table = name;
			continue;
		}
		const equals = line.indexOf('=');
		if (equals === -1) continue;
		const key = trim(line.slice(0, equals)).replace(/^"+|"+$/g, '');
		if (key === '') continue;
		spans.push({
			start: offset + equals + 1,
			end: offset + equals + 1 + valueLength(line.slice(equals + 1), '#'),
			path: table === '' ? key : `${table}.${key}`,
		});
	}
	return spans;
}

function tomlHeader(trimmed: string): string | undefined {
	if (!(trimmed.startsWith('[') && trimmed.endsWith(']')) || trimmed.length < 2)
		return undefined;
	const inner = trimmed.slice(1, -1);
	const nested =
		inner.startsWith('[') && inner.endsWith(']') && inner.length >= 2
			? inner.slice(1, -1)
			: inner;
	return trim(nested);
}

// ----------------------------------------------------------------- INI

function iniSpans(text: string): KeySpan[] {
	let section = '';
	const spans: KeySpan[] = [];
	for (const [offset, line] of lines(text)) {
		const trimmed = trim(line);
		if (trimmed === '' || trimmed.startsWith(';') || trimmed.startsWith('#'))
			continue;
		if (
			trimmed.startsWith('[') &&
			trimmed.endsWith(']') &&
			trimmed.length >= 2
		) {
			section = trim(trimmed.slice(1, -1));
			continue;
		}
		const equals = line.indexOf('=');
		const separator = equals === -1 ? line.indexOf(':') : equals;
		if (separator === -1) continue;
		const key = trim(line.slice(0, separator));
		if (key === '') continue;
		spans.push({
			start: offset + separator + 1,
			end:
				offset + separator + 1 + valueLength(line.slice(separator + 1), ';#'),
			path: section === '' ? key : `${section}.${key}`,
		});
	}
	return spans;
}

// -------------------------------------------------------------- dotenv

function dotenvSpans(text: string): KeySpan[] {
	const spans: KeySpan[] = [];
	for (const [offset, line] of lines(text)) {
		const trimmed = trimStart(line);
		if (trimmed === '' || trimmed.startsWith('#')) continue;
		const equals = line.indexOf('=');
		if (equals === -1) continue;
		const declared = trim(line.slice(0, equals));
		const key = trim(
			declared.startsWith('export ')
				? declared.slice('export '.length)
				: declared,
		);
		if (key === '') continue;
		spans.push({
			start: offset + equals + 1,
			end: offset + equals + 1 + valueLength(line.slice(equals + 1), '#'),
			path: key,
		});
	}
	return spans;
}

// ----------------------------------------------------------------- CSV

function csvSpans(text: string, delimiter: string): KeySpan[] {
	const rows = lines(text).filter(([, line]) => trim(line) !== '');
	const first = rows.shift();
	if (!first) return [];
	const [, headerLine] = first;
	const headers = fields(headerLine, delimiter).map(([start, end]) =>
		trim(headerLine.slice(start, end)).replace(/^"+|"+$/g, ''),
	);
	const spans: KeySpan[] = [];
	for (const [offset, line] of rows) {
		fields(line, delimiter).forEach(([start, end], column) => {
			const name = headers[column];
			spans.push({
				start: offset + start,
				end: offset + end,
				path: name ? name : `[${column}]`,
			});
		});
	}
	return spans;
}

function fields(line: string, delimiter: string): Array<[number, number]> {
	const out: Array<[number, number]> = [];
	let start = 0;
	let quoted = false;
	for (let at = 0; at < line.length; at++) {
		const character = line.charAt(at);
		if (character === '"') quoted = !quoted;
		else if (character === delimiter && !quoted) {
			out.push([start, at]);
			start = at + 1;
		}
	}
	out.push([start, line.length]);
	return out;
}
