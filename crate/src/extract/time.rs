//! Epoch milliseconds → an ISO-8601 UTC string.
//!
//! Hand-written rather than delegated, and the reason is the same one
//! that keeps a date library out of `Cargo.toml`: the whole conversion
//! is one well-known algorithm over integers, it has no timezone
//! database behind it because UTC has no rules to look up, and a
//! dependency here would be a moving part under the single claim this
//! crate exists to make.
//!
//! `days_from_civil` / `civil_from_days` are Howard Hinnant's, which is
//! the same arithmetic every date library uses underneath. Everything is
//! `i64` end to end so there is no cast to get wrong, and dates before
//! 1970 come out right because the division is Euclidean rather than
//! truncating.

const MS_PER_DAY: i64 = 86_400_000;
const MS_PER_HOUR: i64 = 3_600_000;
const MS_PER_MINUTE: i64 = 60_000;
const MS_PER_SECOND: i64 = 1_000;

/// Days from 1970-01-01 to a civil date, and its inverse.
///
/// The era arithmetic shifts the year to start in March so the leap day
/// lands at the end of a 146097-day, 400-year cycle and every month
/// length becomes a linear formula.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// An instant, written the one way every consumer of this report can
/// parse without being told which format it is in.
///
/// Milliseconds are always present, including `.000`. A decoded
/// timestamp is being compared against another timestamp far more often
/// than it is being read aloud, and a field that sometimes disappears is
/// a field every reader has to write a branch for.
pub(crate) fn iso8601_utc(milliseconds: i64) -> String {
    let days = milliseconds.div_euclid(MS_PER_DAY);
    let within_day = milliseconds.rem_euclid(MS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let hour = within_day / MS_PER_HOUR;
    let minute = (within_day % MS_PER_HOUR) / MS_PER_MINUTE;
    let second = (within_day % MS_PER_MINUTE) / MS_PER_SECOND;
    let milli = within_day % MS_PER_SECOND;
    format!(
        "{}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milli:03}Z",
        year_field(year)
    )
}

/// The year, in ISO-8601's expanded representation where it does not fit
/// four digits.
///
/// Four digits is the ordinary case and is untouched — every identifier a
/// document actually holds lands there. Outside it, four digits is not a
/// short ISO-8601 year, it is **not ISO-8601 at all**: the standard asks
/// for a sign and an agreed number of extra digits. This crate agrees on
/// six, which is what `Date.toISOString` writes and therefore what a
/// consumer of a JSON report is most likely to already parse.
///
/// The case is not hypothetical. **The largest instant any kind here can
/// decode is the year 10889** — a ULID or a UUID v7 whose 48-bit
/// millisecond field is all ones, both landing on
/// `+010889-08-02T05:31:50.655Z`. (v1 and v6 read 60 bits of
/// 100-nanosecond ticks from 1582 and top out four digits earlier, in
/// 5236; ObjectId's 32-bit seconds end in 2106 and a Snowflake's 42 bits
/// of milliseconds around 2149.) Every one of those arrives on a
/// `timestamp_implausible` refusal, where the decode is the evidence the
/// reader is being asked to check — so six digits is comfortably enough,
/// and a year that somehow needed more would get them rather than be
/// truncated into a wrong number.
fn year_field(year: i64) -> String {
    if (0..=9999).contains(&year) {
        return format!("{year:04}");
    }
    let sign = if year < 0 { '-' } else { '+' };
    // `unsigned_abs`, not `abs`: overflow-checks are on in release and
    // this must not be the one place a report panics.
    format!("{sign}{:06}", year.unsigned_abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unix_epoch_is_the_epoch() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn milliseconds_are_always_written() {
        assert_eq!(iso8601_utc(1), "1970-01-01T00:00:00.001Z");
        assert_eq!(iso8601_utc(1_000), "1970-01-01T00:00:01.000Z");
    }

    /// Pinned against dates computed independently, so this is a check
    /// on the arithmetic rather than a restatement of it.
    #[test]
    fn known_instants_round_trip() {
        for (milliseconds, expected) in [
            (631_152_000_000, "1990-01-01T00:00:00.000Z"),
            (946_684_800_000, "2000-01-01T00:00:00.000Z"),
            (951_782_400_000, "2000-02-29T00:00:00.000Z"),
            (1_288_834_974_657, "2010-11-04T01:42:54.657Z"),
            (1_420_070_400_000, "2015-01-01T00:00:00.000Z"),
            (1_786_492_800_000, "2026-08-12T00:00:00.000Z"),
        ] {
            assert_eq!(iso8601_utc(milliseconds), expected, "{milliseconds}");
        }
    }

    /// A UUID v1 clock predates the epoch by nearly four centuries, so a
    /// negative instant is not a defensive case here — it is what a v1
    /// with a zeroed time field decodes to, and a truncating division
    /// would put it on the wrong day.
    #[test]
    fn an_instant_before_the_epoch_is_not_off_by_a_day() {
        assert_eq!(iso8601_utc(-1), "1969-12-31T23:59:59.999Z");
        assert_eq!(iso8601_utc(-MS_PER_DAY), "1969-12-31T00:00:00.000Z");
        assert_eq!(
            iso8601_utc(-12_219_292_800_000),
            "1582-10-15T00:00:00.000Z",
            "the Gregorian reform, which is where the v1 clock starts"
        );
    }

    /// **ISO-8601's expanded form, because four digits are not enough.**
    /// A ULID or a UUID v7 at the top of its 48-bit millisecond field
    /// decodes to the year 10889 — the largest instant any kind here can
    /// reach. Written as bare digits that is not ISO-8601, and it rides
    /// on a refusal a reader is meant to be able to check.
    ///
    /// The boundary constant is counted year by year in the test's own
    /// terms rather than taken from `civil_from_days`, so this is a check
    /// on the arithmetic and not a restatement of it.
    #[test]
    fn a_year_outside_four_digits_is_written_in_expanded_form() {
        /// 1970-01-01 to 10000-01-01, summing 365 or 366 per year.
        const YEAR_10000: i64 = 253_402_300_800_000;

        assert_eq!(iso8601_utc(YEAR_10000 - 1), "9999-12-31T23:59:59.999Z");
        assert_eq!(iso8601_utc(YEAR_10000), "+010000-01-01T00:00:00.000Z");
        // The ceiling of the whole crate, stated so the doc above it has
        // something behind it: 2^48 - 1 milliseconds is the largest value
        // any kind's time field can hold, from a ULID or a UUID v7. v1
        // and v6 stop in 5236, ObjectId in 2106, Snowflake around 2149.
        assert_eq!(
            iso8601_utc((1 << 48) - 1),
            "+010889-08-02T05:31:50.655Z",
            "the largest instant any kind here can decode"
        );
    }

    /// A year before 0000 is not reachable from any identifier here — the
    /// oldest clock is UUID v1's, and a zeroed one lands in 1582 — but
    /// the sign is required in both directions and costs nothing to get
    /// right.
    #[test]
    fn a_year_before_the_common_era_carries_its_sign() {
        const YEAR_ZERO: i64 = -62_167_219_200_000;
        assert_eq!(iso8601_utc(YEAR_ZERO), "0000-01-01T00:00:00.000Z");
        assert_eq!(iso8601_utc(YEAR_ZERO - 1), "-000001-12-31T23:59:59.999Z");
    }

    /// Every month boundary in a leap year and the year after it. The
    /// month-length formula is the part of this algorithm that fails
    /// silently, and it fails on exactly one day.
    #[test]
    fn every_month_boundary_lands_on_the_first() {
        let mut days = (2024 - 1970) * 365 + 13; // 2024-01-01, 13 leap days behind it
        for (year, month, length) in [
            (2024, 1, 31),
            (2024, 2, 29),
            (2024, 3, 31),
            (2024, 4, 30),
            (2024, 5, 31),
            (2024, 6, 30),
            (2024, 7, 31),
            (2024, 8, 31),
            (2024, 9, 30),
            (2024, 10, 31),
            (2024, 11, 30),
            (2024, 12, 31),
            (2025, 1, 31),
            (2025, 2, 28),
        ] {
            assert_eq!(
                iso8601_utc(days * MS_PER_DAY),
                format!("{year:04}-{month:02}-01T00:00:00.000Z")
            );
            days += length;
        }
    }
}
