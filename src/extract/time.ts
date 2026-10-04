/**
 * Unix milliseconds as ISO-8601 UTC — the crate's `time.rs`.
 *
 * Not `Date.toISOString`: the crate formats with its own civil-from-days
 * arithmetic, and a year outside 0000–9999 must come out in the expanded
 * representation the same way on both surfaces.
 */
const MS_PER_DAY = 86_400_000;
const MS_PER_HOUR = 3_600_000;
const MS_PER_MINUTE = 60_000;
const MS_PER_SECOND = 1_000;

const floorDiv = (a: number, b: number): number => Math.floor(a / b);
const euclidMod = (a: number, b: number): number => a - b * floorDiv(a, b);

function civilFromDays(days: number): [number, number, number] {
	const shifted = days + 719_468;
	const era = floorDiv(shifted, 146_097);
	const dayOfEra = shifted - era * 146_097;
	const yearOfEra = Math.trunc(
		(dayOfEra -
			Math.trunc(dayOfEra / 1_460) +
			Math.trunc(dayOfEra / 36_524) -
			Math.trunc(dayOfEra / 146_096)) /
			365,
	);
	const year = yearOfEra + era * 400;
	const dayOfYear =
		dayOfEra -
		(365 * yearOfEra + Math.trunc(yearOfEra / 4) - Math.trunc(yearOfEra / 100));
	const shiftedMonth = Math.trunc((5 * dayOfYear + 2) / 153);
	const day = dayOfYear - Math.trunc((153 * shiftedMonth + 2) / 5) + 1;
	const month = shiftedMonth < 10 ? shiftedMonth + 3 : shiftedMonth - 9;
	return [month <= 2 ? year + 1 : year, month, day];
}

const pad = (value: number, width: number): string =>
	String(value).padStart(width, '0');

export function iso8601(milliseconds: number): string {
	const days = floorDiv(milliseconds, MS_PER_DAY);
	const withinDay = euclidMod(milliseconds, MS_PER_DAY);
	const [year, month, day] = civilFromDays(days);
	const hour = Math.trunc(withinDay / MS_PER_HOUR);
	const minute = Math.trunc((withinDay % MS_PER_HOUR) / MS_PER_MINUTE);
	const second = Math.trunc((withinDay % MS_PER_MINUTE) / MS_PER_SECOND);
	const milli = withinDay % MS_PER_SECOND;
	return `${yearField(year)}-${pad(month, 2)}-${pad(day, 2)}T${pad(hour, 2)}:${pad(minute, 2)}:${pad(second, 2)}.${pad(milli, 3)}Z`;
}

function yearField(year: number): string {
	if (year >= 0 && year <= 9999) return pad(year, 4);
	return `${year < 0 ? '-' : '+'}${pad(Math.abs(year), 6)}`;
}
