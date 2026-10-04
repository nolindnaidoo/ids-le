import type { Reader } from './locate';
import { trim } from './text';

/**
 * Which key-path reader a document gets — the crate's `format.rs`.
 *
 * An unresolved format is not an error: the identifier scan is the same for
 * every document, and the format decides only whether a finding can carry a
 * key path. `conf` and `cfg` are deliberately absent — the INI reader read
 * `key: value` out of prose and let an English sentence decide a verdict.
 */
const ALIASES: ReadonlyArray<readonly [string, Reader]> = Object.freeze([
	['json', 'json'],
	['jsonc', 'json'],
	['yaml', 'yaml'],
	['yml', 'yaml'],
	['csv', 'csv'],
	['tsv', 'tsv'],
	['toml', 'toml'],
	['ini', 'ini'],
	['properties', 'ini'],
	['env', 'env'],
	['dotenv', 'env'],
	['text', 'text'],
	['txt', 'text'],
]);

export const SUPPORTED_FORMATS = Object.freeze([
	'json',
	'yaml',
	'csv',
	'tsv',
	'toml',
	'ini',
	'env',
	'text',
]);
export const FALLBACK_FORMAT = 'text';

function normalise(value: string): string {
	return trim(value).replace(/^\.+/, '').toLowerCase();
}

/** The reader for a canonical name, or the plain-text fallback. */
export function canonical(format: string): Reader {
	return ALIASES.find(([alias]) => alias === format)?.[1] ?? 'text';
}

/** From an explicit format, else from a filename, else the fallback. */
export function resolveFormat(
	format: string | undefined,
	filename: string | undefined,
): Reader {
	if (format !== undefined) {
		const direct = canonical(normalise(format));
		if (direct !== 'text') return direct;
	}
	if (filename === undefined) return 'text';
	const whole = canonical(normalise(filename));
	if (whole !== 'text') return whole;
	if (isDotenv(trim(filename).toLowerCase())) return 'env';
	const dot = filename.lastIndexOf('.');
	return dot === -1 ? 'text' : canonical(normalise(filename.slice(dot + 1)));
}

/** `.env` and anything after it, plus `<name>.env`. `env.ts` is not one. */
function isDotenv(name: string): boolean {
	return (
		name === '.env' ||
		name.startsWith('.env.') ||
		name === 'env' ||
		(name.endsWith('.env') && name.length > 4)
	);
}
