import { iso8601 } from './time';

/**
 * The one classification policy every kind shares, and the router that sends a
 * candidate to its kind — the crate's `policy.rs`.
 *
 * **Refusing is the product.** A refusal is a row: the raw text, its position,
 * a named reason and whatever was decoded before the refusal.
 */

export const KIND_NAMES = Object.freeze([
	'uuid',
	'ulid',
	'nanoid',
	'objectid',
	'snowflake',
] as const);
export type Kind = (typeof KIND_NAMES)[number];

export type Variant = 'ncs' | 'rfc4122' | 'microsoft' | 'future';

export type Reason =
	| 'ambiguous_kind'
	| 'malformed'
	| 'nil_or_max'
	| 'version_claim_mismatch'
	| 'timestamp_implausible';

export interface Decoded {
	readonly version?: number;
	readonly variant?: Variant;
	readonly timestamp?: string;
}

export type Verdict =
	| { readonly verdict: 'ignored' }
	| ({ readonly verdict: 'named'; readonly kind: Kind } & Decoded)
	| ({
			readonly verdict: 'refused';
			readonly kind: Kind | null;
			readonly reason: Reason;
			readonly detail: string;
	  } & Decoded);

export const IGNORED: Verdict = Object.freeze({ verdict: 'ignored' });

export function named(kind: Kind, decoded: Decoded = {}): Verdict {
	return { verdict: 'named', kind, ...decoded };
}

export function refused(
	kind: Kind | null,
	reason: Reason,
	detail: string,
	decoded: Decoded = {},
): Verdict {
	return { verdict: 'refused', kind, reason, detail, ...decoded };
}

/** 1990-01-01T00:00:00Z. */
export const EARLIEST_PLAUSIBLE_MS = 631_152_000_000;
const FUTURE_ALLOWANCE_MS = 365 * 86_400_000;

/** Now, passed in rather than read: the surfaces read the clock. */
export interface Clock {
	readonly nowMs: number;
}

export function isPlausible(clock: Clock, milliseconds: number): boolean {
	return (
		milliseconds >= EARLIEST_PLAUSIBLE_MS &&
		milliseconds <= clock.nowMs + FUTURE_ALLOWANCE_MS
	);
}

/** Each kind is imported lazily by the router to keep this module the policy. */
export interface Classifiers {
	uuid(token: string, clock: Clock): Verdict;
	ulid(token: string, key: string | undefined, clock: Clock): Verdict;
	objectid(token: string, key: string | undefined, clock: Clock): Verdict;
	nanoid(token: string, key: string | undefined): Verdict;
	snowflake(token: string, key: string | undefined, clock: Clock): Verdict;
}

export function route(
	classifiers: Classifiers,
	token: string,
	key: string | undefined,
	clock: Clock,
): Verdict {
	const length = token.length;
	if (length === 36) return classifiers.uuid(token, clock);
	if (length === 26) return classifiers.ulid(token, key, clock);
	if (length === 24) return classifiers.objectid(token, key, clock);
	if (length === 21) return classifiers.nanoid(token, key);
	if (length >= 17 && length <= 19 && /^[0-9]+$/.test(token))
		return classifiers.snowflake(token, key, clock);
	if (length === 32 && isHex(token)) {
		return refused(
			null,
			'ambiguous_kind',
			'32 hex digits are an unhyphenated UUID and an MD5 digest in equal measure; nothing in this document chooses between them',
		);
	}
	if (isNearMissHex(token)) {
		return refused(
			null,
			'malformed',
			`${length} hex digits is not a whole number of bytes and is one digit from a hex identifier this tool knows`,
		);
	}
	return IGNORED;
}

export function isHex(token: string): boolean {
	return /^[0-9A-Fa-f]*$/.test(token);
}

/** One digit either side of 24 or 32, with a letter so a Snowflake reaches its kind. */
function isNearMissHex(token: string): boolean {
	const length = token.length;
	const near = Math.abs(length - 24) === 1 || Math.abs(length - 32) === 1;
	return near && isHex(token) && /[A-Za-z]/.test(token);
}

/** The key path lowercased with everything but ASCII alphanumerics removed. */
export function flattenKey(key: string): string {
	return key.replace(/[^0-9A-Za-z]/g, '').toLowerCase();
}

/** The last segment of a dotted key path. */
export function leafKey(key: string): string {
	return key.slice(key.lastIndexOf('.') + 1);
}

/** Whether the key path mentions a word anywhere along it. */
export function keyMentions(key: string | undefined, word: string): boolean {
	return key !== undefined && flattenKey(key).includes(word);
}

/** Whether the field's own name names a scheme outright. */
export function namesScheme(key: string | undefined, scheme: string): boolean {
	return key !== undefined && flattenKey(leafKey(key)).includes(scheme);
}

/** Whether the field's own name ends in `id`. */
export function namesAnId(key: string | undefined): boolean {
	return key !== undefined && flattenKey(leafKey(key)).endsWith('id');
}

export { iso8601 };
