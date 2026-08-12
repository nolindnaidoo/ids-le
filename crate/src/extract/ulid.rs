//! ULIDs: 26 characters of Crockford base32, the first ten of them a
//! 48-bit Unix millisecond.
//!
//! A ULID has no version field and no checksum, so the only things that
//! can be checked are its alphabet, its length, and whether the time it
//! claims is a time. That thinness is why the ambiguity rule here is
//! about **case**: the specification's canonical form is uppercase, and
//! a 26-character mixed-case alphanumeric run is exactly as likely to be
//! a token from a scheme this crate has never heard of. Uppercase is
//! evidence; mixed case is the absence of it.
//!
//! Lowercase sits in between — several libraries emit it — so it is
//! named only when the document's own key says ULID.

use super::policy::{Clock, Id, Kind, Reason, Refusal, Verdict, names_scheme};
use super::time::iso8601_utc;

/// Crockford base32: the digits, then the letters with `I`, `L`, `O` and
/// `U` removed. `I`/`L` are excluded because they read as `1`, `O`
/// because it reads as `0`, and `U` to keep accidental obscenities out
/// of generated identifiers.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

const LENGTH: usize = 26;
const TIME_CHARS: usize = 10;
/// The timestamp is 48 bits and ten base32 characters hold 50, so the
/// first character can only use three of its five bits. A leading
/// character above `7` is a value no generator can produce.
const MAX_LEADING: u32 = 7;

pub(crate) fn classify(token: &str, key: Option<&str>, clock: Clock) -> Verdict {
    if !is_identifier_shaped(token) {
        return Verdict::Ignored;
    }

    let upper = token.to_ascii_uppercase();
    let Some(values) = decode(&upper) else {
        return Verdict::Refused(Refusal::new(
            Some(Kind::Ulid),
            Reason::Malformed,
            "26 characters is a ULID's length, and Crockford base32 excludes I, L, O and U"
                .to_string(),
        ));
    };
    if values[0] > MAX_LEADING {
        return Verdict::Refused(Refusal::new(
            Some(Kind::Ulid),
            Reason::Malformed,
            format!(
                "the leading character encodes {}, and a 48-bit timestamp cannot exceed {MAX_LEADING} there",
                values[0]
            ),
        ));
    }

    // The kind is settled before the clock is read: an implausible time
    // is a fact about a ULID, and this may not be one.
    let canonical = token.bytes().all(|byte| !byte.is_ascii_lowercase());
    if !canonical && !names_scheme(key, "ulid") {
        return Verdict::Refused(Refusal::new(
            None,
            Reason::AmbiguousKind,
            "a valid ULID is canonically uppercase; in this case it is equally a token from \
             another 26-character scheme, and no key here says which"
                .to_string(),
        ));
    }

    let milliseconds = milliseconds_of(&values);
    let timestamp = iso8601_utc(milliseconds);
    if !clock.is_plausible(milliseconds) {
        return Verdict::Refused(Refusal {
            timestamp: Some(timestamp),
            ..Refusal::new(
                Some(Kind::Ulid),
                Reason::TimestampImplausible,
                "the first ten characters decode outside the plausible range".to_string(),
            )
        });
    }
    Verdict::Named(Id::at(Kind::Ulid, timestamp))
}

/// The gate that keeps ordinary words out of the alphabet check.
///
/// Without it every 26-letter word in a document would be refused as a
/// malformed ULID for containing a vowel, and a report that is mostly
/// refusals about English is a report nobody reads.
fn is_identifier_shaped(token: &str) -> bool {
    token.len() == LENGTH
        && token.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && token.bytes().any(|byte| byte.is_ascii_digit())
        && token.bytes().any(|byte| byte.is_ascii_alphabetic())
}

/// Each character's value, or nothing if one of them is not in the
/// alphabet. The caller has already uppercased.
fn decode(upper: &str) -> Option<Vec<u32>> {
    upper
        .bytes()
        .map(|byte| {
            ALPHABET
                .iter()
                .position(|candidate| *candidate == byte)
                .and_then(|index| u32::try_from(index).ok())
        })
        .collect()
}

fn milliseconds_of(values: &[u32]) -> i64 {
    values[..TIME_CHARS]
        .iter()
        .fold(0i64, |total, value| total * 32 + i64::from(*value))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: Clock = Clock::at(1_786_492_800_000);
    /// 2026-08-12T00:00:00.000Z in the first ten characters.
    const CANONICAL: &str = "01KZSM9K00ABCDEFGH12345678";

    fn classify_bare(token: &str) -> Verdict {
        classify(token, None, CLOCK)
    }

    fn named(token: &str, key: Option<&str>) -> Id {
        match classify(token, key, CLOCK) {
            Verdict::Named(id) => id,
            other => panic!("{token} was {other:?}, not a named ULID"),
        }
    }

    fn refused(token: &str, key: Option<&str>) -> Refusal {
        match classify(token, key, CLOCK) {
            Verdict::Refused(refusal) => refusal,
            other => panic!("{token} was {other:?}, not a refusal"),
        }
    }

    #[test]
    fn a_canonical_ulid_is_named_with_its_timestamp() {
        assert_eq!(
            named(CANONICAL, None).timestamp,
            Some("2026-08-12T00:00:00.000Z".to_string())
        );
    }

    /// The epoch and the ceiling of the time field, so the decode is
    /// pinned at both ends rather than at one convenient point.
    #[test]
    fn the_time_field_decodes_at_both_of_its_limits() {
        assert_eq!(
            milliseconds_of(&decode("0000000000").expect("valid")),
            0,
            "ten zeros are the Unix epoch"
        );
        assert_eq!(
            milliseconds_of(&decode("7ZZZZZZZZZ").expect("valid")),
            281_474_976_710_655,
            "the largest 48-bit millisecond"
        );
    }

    /// The alphabet is the only structural check a ULID has, so it is
    /// the only thing `malformed` can mean here.
    #[test]
    fn a_character_outside_crockford_is_malformed() {
        for excluded in ['I', 'L', 'O', 'U'] {
            let token = format!("01KZSM9K00ABCDEFGH123456{excluded}8");
            let refusal = refused(&token, None);
            assert_eq!(refusal.reason, Reason::Malformed, "{token}");
            assert_eq!(refusal.kind, Some(Kind::Ulid));
        }
    }

    #[test]
    fn a_leading_character_the_time_field_cannot_reach_is_malformed() {
        let refusal = refused("81KZSM9K00ABCDEFGH12345678", None);
        assert_eq!(refusal.reason, Reason::Malformed);
        assert!(refusal.detail.contains("48-bit"), "{}", refusal.detail);
    }

    /// The refusal this crate exists to demonstrate: structurally
    /// perfect, and nothing in the document says which scheme it is.
    #[test]
    fn a_non_canonical_case_is_ambiguous_rather_than_guessed() {
        let refusal = refused("01kzsm9k00AbCdEfGh12345678", None);
        assert_eq!(refusal.reason, Reason::AmbiguousKind);
        assert_eq!(refusal.kind, None, "no kind was named");
    }

    #[test]
    fn a_lowercase_ulid_is_named_when_the_key_says_it_is_one() {
        assert_eq!(
            named("01kzsm9k00abcdefgh12345678", Some("session.ulid")).timestamp,
            Some("2026-08-12T00:00:00.000Z".to_string())
        );
        assert_eq!(
            refused("01kzsm9k00abcdefgh12345678", Some("session.token")).reason,
            Reason::AmbiguousKind
        );
    }

    /// **The leaf, not the whole path** — the same rule ObjectId,
    /// Snowflake and NanoID are held to. A count that happens to sit in a
    /// table about ULIDs is still a count, and the table's name is not
    /// the document calling *this field* a ULID.
    #[test]
    fn only_the_leaf_of_the_key_path_names_the_scheme() {
        assert_eq!(
            refused("01kzsm9k00abcdefgh12345678", Some("ulids.count")).reason,
            Reason::AmbiguousKind
        );
        assert_eq!(
            named("01kzsm9k00abcdefgh12345678", Some("ulids.session_ulid")).timestamp,
            Some("2026-08-12T00:00:00.000Z".to_string())
        );
    }

    #[test]
    fn an_implausible_timestamp_reports_the_decode_and_the_flag() {
        let refusal = refused("7ZZZZZZZZZABCDEFGH12345678", None);
        assert_eq!(refusal.reason, Reason::TimestampImplausible);
        assert_eq!(
            refusal.timestamp,
            Some("10889-08-02T05:31:50.655Z".to_string())
        );
    }

    /// Without the shape gate this would be a refusal about a word,
    /// which is the failure mode that makes a report unreadable.
    #[test]
    fn a_twenty_six_letter_word_is_ignored_rather_than_refused() {
        assert_eq!(
            classify_bare("abcdefghijklmnopqrstuvwxyz"),
            Verdict::Ignored
        );
        assert_eq!(
            classify_bare("ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
            Verdict::Ignored,
            "no digit, so nothing claims to be a ULID"
        );
    }

    #[test]
    fn a_run_of_digits_alone_is_not_a_ulid() {
        assert_eq!(
            classify_bare("01234567890123456789012345"),
            Verdict::Ignored
        );
    }
}
