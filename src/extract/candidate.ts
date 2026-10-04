/**
 * Every run of text that could be an identifier, with where it starts —
 * the crate's `candidate.rs`.
 *
 * A candidate is a **maximal** run over `[0-9A-Za-z_-]`: `user_550e8400-…` is
 * one 45-character run that matches no shape, never a prefix beside a UUID.
 */
const SHORTEST = 17;
const LONGEST = 36;

export interface Candidate {
	readonly text: string;
	/** UTF-16 index of the first character. */
	readonly start: number;
}

/** Runs are ASCII, so a code unit is a byte and lengths agree with the crate's. */
function isRunUnit(unit: number): boolean {
	return (
		(unit >= 0x30 && unit <= 0x39) ||
		(unit >= 0x41 && unit <= 0x5a) ||
		(unit >= 0x61 && unit <= 0x7a) ||
		unit === 0x5f ||
		unit === 0x2d
	);
}

export function candidates(text: string): Candidate[] {
	const out: Candidate[] = [];
	let at = 0;
	while (at < text.length) {
		if (!isRunUnit(text.charCodeAt(at))) {
			at++;
			continue;
		}
		const start = at;
		while (at < text.length && isRunUnit(text.charCodeAt(at))) at++;
		const length = at - start;
		if (length >= SHORTEST && length <= LONGEST)
			out.push({ text: text.slice(start, at), start });
	}
	return out;
}
