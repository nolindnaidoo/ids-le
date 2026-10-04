import {
	type Classifiers,
	type Clock,
	IGNORED,
	isHex,
	iso8601,
	isPlausible,
	keyMentions,
	named,
	namesAnId,
	namesScheme,
	refused,
	type Variant,
	type Verdict,
} from './policy';

/**
 * The five kinds — the crate's `uuid.rs`, `ulid.rs`, `nanoid.rs`,
 * `objectid.rs` and `snowflake.rs`, one function each, in that order.
 */

// ---------------------------------------------------------------- UUID

const HYPHENS = new Set([8, 13, 18, 23]);
const VERSION_NIBBLE = 12;
const VARIANT_NIBBLE = 16;
const GREGORIAN_OFFSET_TICKS = 122_192_928_000_000_000n;
const TICKS_PER_MILLISECOND = 10_000n;
const SUSPICIOUS_ZERO_RUN = 12;

function uuid(token: string, clock: Clock): Verdict {
	const hex = stripHyphens(token);
	if (hex === undefined) return IGNORED;
	if (!isHex(hex)) {
		return refused(
			'uuid',
			'malformed',
			'the 8-4-4-4-12 shape is right and at least one character is not a hex digit',
		);
	}
	if (/^0+$/.test(hex)) {
		return refused(
			'uuid',
			'nil_or_max',
			'the nil UUID: 128 zero bits, which RFC 9562 defines as naming nothing',
		);
	}
	if (/^[fF]+$/.test(hex)) {
		return refused(
			'uuid',
			'nil_or_max',
			'the max UUID: 128 one bits, which RFC 9562 defines as naming nothing',
		);
	}

	const version = Number.parseInt(hex.charAt(VERSION_NIBBLE), 16);
	const variant = variantOf(Number.parseInt(hex.charAt(VARIANT_NIBBLE), 16));
	if (variant !== 'rfc4122') {
		return refused(
			'uuid',
			'malformed',
			`the variant bits say ${VARIANT_NAMES[variant]}, not RFC 4122, so the version nibble is not a version`,
			{ variant },
		);
	}
	if (version < 1 || version > 8) {
		return refused(
			'uuid',
			'malformed',
			`RFC 9562 defines versions 1 through 8; this one claims ${version}`,
			{
				variant,
			},
		);
	}
	const zeros = longestZeroRun(hex);
	if (version === 4 && zeros >= SUSPICIOUS_ZERO_RUN) {
		return refused(
			'uuid',
			'version_claim_mismatch',
			`the version nibble claims v4, which is 122 random bits, and ${zeros} of its nibbles in a row are zero`,
			{ version, variant },
		);
	}

	const milliseconds = uuidTimestamp(hex, version);
	if (milliseconds === undefined) return named('uuid', { version, variant });
	const timestamp = iso8601(milliseconds);
	if (!isPlausible(clock, milliseconds)) {
		return refused(
			'uuid',
			'timestamp_implausible',
			`the v${version} time field decodes outside the plausible range`,
			{
				version,
				variant,
				timestamp,
			},
		);
	}
	return named('uuid', { version, variant, timestamp });
}

function stripHyphens(token: string): string | undefined {
	if (token.length !== 36) return undefined;
	let hex = '';
	for (let index = 0; index < 36; index++) {
		const character = token.charAt(index);
		if (HYPHENS.has(index)) {
			if (character !== '-') return undefined;
			continue;
		}
		if (!/[0-9A-Za-z]/.test(character)) return undefined;
		hex += character;
	}
	return hex;
}

function variantOf(nibble: number): Variant {
	if (nibble <= 7) return 'ncs';
	if (nibble <= 11) return 'rfc4122';
	if (nibble <= 13) return 'microsoft';
	return 'future';
}

const VARIANT_NAMES: Readonly<Record<Variant, string>> = Object.freeze({
	ncs: 'NCS',
	rfc4122: 'RFC 4122',
	microsoft: 'Microsoft',
	future: 'reserved-future',
});

/** The longest zero-nibble run, with the version and variant nibbles masked out. */
function longestZeroRun(hex: string): number {
	let longest = 0;
	let current = 0;
	for (let index = 0; index < hex.length; index++) {
		if (
			index === VERSION_NIBBLE ||
			index === VARIANT_NIBBLE ||
			hex.charAt(index) !== '0'
		) {
			current = 0;
			continue;
		}
		current++;
		longest = Math.max(longest, current);
	}
	return longest;
}

const field = (hex: string, from: number, to: number): bigint =>
	BigInt(`0x${hex.slice(from, to)}`);

/** v2 is deliberately not decoded: its clock has a POSIX uid in its low 32 bits. */
function uuidTimestamp(hex: string, version: number): number | undefined {
	if (version === 1) {
		return fromGregorian(
			((field(hex, 12, 16) & 0x0fffn) << 48n) |
				(field(hex, 8, 12) << 32n) |
				field(hex, 0, 8),
		);
	}
	if (version === 6) {
		return fromGregorian(
			(field(hex, 0, 8) << 28n) |
				(field(hex, 8, 12) << 12n) |
				(field(hex, 12, 16) & 0x0fffn),
		);
	}
	if (version === 7) return Number(field(hex, 0, 12));
	return undefined;
}

/** Euclidean, as `div_euclid`: a zeroed field lands before the epoch. */
function fromGregorian(ticks: bigint): number {
	const delta = ticks - GREGORIAN_OFFSET_TICKS;
	let quotient = delta / TICKS_PER_MILLISECOND;
	if (delta % TICKS_PER_MILLISECOND < 0n) quotient -= 1n;
	return Number(quotient);
}

// ---------------------------------------------------------------- ULID

const CROCKFORD = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
const MAX_LEADING = 7;

function ulid(token: string, key: string | undefined, clock: Clock): Verdict {
	if (
		!(
			token.length === 26 &&
			/^[0-9A-Za-z]+$/.test(token) &&
			/[0-9]/.test(token) &&
			/[A-Za-z]/.test(token)
		)
	) {
		return IGNORED;
	}
	const values = Array.from(token.toUpperCase(), (character) =>
		CROCKFORD.indexOf(character),
	);
	if (values.some((value) => value === -1)) {
		return refused(
			'ulid',
			'malformed',
			"26 characters is a ULID's length, and Crockford base32 excludes I, L, O and U",
		);
	}
	const leading = values[0] as number;
	if (leading > MAX_LEADING) {
		return refused(
			'ulid',
			'malformed',
			`the leading character encodes ${leading}, and a 48-bit timestamp cannot exceed ${MAX_LEADING} there`,
		);
	}
	const canonical = !/[a-z]/.test(token);
	if (!canonical && !namesScheme(key, 'ulid')) {
		return refused(
			null,
			'ambiguous_kind',
			'a valid ULID is canonically uppercase; in this case it is equally a token from another 26-character scheme, and no key here says which',
		);
	}
	const milliseconds = values
		.slice(0, 10)
		.reduce((total, value) => total * 32 + value, 0);
	const timestamp = iso8601(milliseconds);
	if (!isPlausible(clock, milliseconds)) {
		return refused(
			'ulid',
			'timestamp_implausible',
			'the first ten characters decode outside the plausible range',
			{
				timestamp,
			},
		);
	}
	return named('ulid', { timestamp });
}

// -------------------------------------------------------------- NanoID

function nanoid(token: string, key: string | undefined): Verdict {
	if (
		!(
			token.length === 21 &&
			/[0-9]/.test(token) &&
			/[A-Z]/.test(token) &&
			/[a-z]/.test(token)
		)
	)
		return IGNORED;
	const base64url = /[-_]/.test(token);
	if (!base64url && !namesScheme(key, 'nanoid')) {
		return refused(
			null,
			'ambiguous_kind',
			'21 base62 characters are a NanoID, a short token and a truncated hash in equal measure; only a `-` or `_`, or a key naming the scheme, separates them',
		);
	}
	return named('nanoid');
}

// ------------------------------------------------------------ ObjectId

function objectid(
	token: string,
	key: string | undefined,
	clock: Clock,
): Verdict {
	if (token.length !== 24 || !isHex(token)) return IGNORED;
	const milliseconds = Number.parseInt(token.slice(0, 8), 16) * 1_000;
	const timestamp = iso8601(milliseconds);
	if (!namesAnId(key)) {
		return refused(
			null,
			'ambiguous_kind',
			'24 hex digits are an ObjectId, a truncated hash and a hex dump in equal measure, and nothing in this document names this field an identifier',
			{ timestamp },
		);
	}
	if (!isPlausible(clock, milliseconds)) {
		return refused(
			null,
			'ambiguous_kind',
			'the field is named like an identifier, and 24 hex digits are an ObjectId only if the leading four bytes are also a time; these are not, and a truncated hash fits the same shape',
			{ timestamp },
		);
	}
	return named('objectid', { timestamp });
}

// ----------------------------------------------------------- Snowflake

const TWITTER_EPOCH_MS = 1_288_834_974_657;
const DISCORD_EPOCH_MS = 1_420_070_400_000;
const DISCORD_WORDS = Object.freeze(['discord', 'guild', 'channel', 'message']);
const TWITTER_WORDS = Object.freeze(['twitter', 'tweet', 'status']);

function snowflake(
	token: string,
	key: string | undefined,
	clock: Clock,
): Verdict {
	if (!(namesAnId(key) || namesScheme(key, 'snowflake'))) return IGNORED;
	const shifted = Number(BigInt(token) >> 22n);
	const discord = shifted + DISCORD_EPOCH_MS;
	const twitter = shifted + TWITTER_EPOCH_MS;

	const saysDiscord = DISCORD_WORDS.some((word) => keyMentions(key, word));
	const saysTwitter = TWITTER_WORDS.some((word) => keyMentions(key, word));
	let milliseconds: number | undefined;
	if (saysDiscord && !saysTwitter) milliseconds = discord;
	else if (saysTwitter && !saysDiscord) milliseconds = twitter;
	else milliseconds = byClock(discord, twitter, clock);

	if (milliseconds === undefined) {
		return refused(
			'snowflake',
			'ambiguous_kind',
			`the Twitter epoch decodes this to ${iso8601(twitter)} and the Discord epoch to ${iso8601(discord)}; both are plausible and no key here names the platform`,
			{ timestamp: iso8601(twitter) },
		);
	}
	const timestamp = iso8601(milliseconds);
	if (!isPlausible(clock, milliseconds)) {
		return refused(
			'snowflake',
			'timestamp_implausible',
			'the top 42 bits decode outside the plausible range under either epoch',
			{ timestamp },
		);
	}
	return named('snowflake', { timestamp });
}

/** The epoch the clock chose where nothing in the document did. */
function byClock(
	discord: number,
	twitter: number,
	clock: Clock,
): number | undefined {
	const discordFits = isPlausible(clock, discord);
	if (discordFits && isPlausible(clock, twitter)) return undefined;
	if (discordFits) return discord;
	return twitter;
}

export const CLASSIFIERS: Classifiers = Object.freeze({
	uuid,
	ulid,
	objectid,
	nanoid,
	snowflake,
});
