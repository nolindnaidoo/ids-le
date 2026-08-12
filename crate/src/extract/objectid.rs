//! MongoDB ObjectIds: twelve bytes as 24 hex digits, the first four of
//! them a Unix second.
//!
//! An ObjectId has no version, no variant, no checksum and no reserved
//! bits. Twenty-four hex digits are twenty-four hex digits — a truncated
//! SHA-1, the front of a git object name, a hex dump of anything at all.
//! **The embedded timestamp is the only evidence in the string that it
//! is an ObjectId**, so this module makes the decode the test: a leading
//! four bytes that land in the plausible range are an ObjectId, and one
//! that does not is refused as `ambiguous_kind`.
//!
//! That is deliberately not `timestamp_implausible`. Elsewhere — a UUID
//! with a version nibble, a canonically-cased ULID — the shape says what
//! the thing is and only the time is wrong. Here the time *is* the
//! claim, so a bad one unmakes the kind rather than the timestamp.
//!
//! The residual cost is stated rather than hidden: the plausible window
//! covers 1,186,876,800 of the 4,294,967,296 seconds a 32-bit field can
//! hold, so about one random 24-hex run in four reaches this module and
//! is named — 163 of 600 abbreviated SHA-1 and MD5 digests, measured on
//! `fixtures/documents/hashes.txt` by `tests/coverage_matrix.rs`, which
//! prints the rate on every run. That is why the decoded timestamp is on
//! the row: a reader looking at a "2003-11-08" ObjectId in a document
//! written last week has what they need to disagree. SPEC.md carries the
//! arithmetic.

use super::policy::{Clock, Id, Kind, Reason, Refusal, Verdict, is_hex};
use super::time::iso8601_utc;

const LENGTH: usize = 24;
/// The four bytes of Unix seconds at the front.
const TIME_CHARS: usize = 8;

pub(crate) fn classify(token: &str, clock: Clock) -> Verdict {
    if token.len() != LENGTH || !is_hex(token) {
        return Verdict::Ignored;
    }

    let seconds =
        i64::from_str_radix(&token[..TIME_CHARS], 16).expect("eight checked hex digits fit i64");
    let milliseconds = seconds * 1_000;
    let timestamp = iso8601_utc(milliseconds);

    if !clock.is_plausible(milliseconds) {
        return Verdict::Refused(Refusal {
            timestamp: Some(timestamp),
            ..Refusal::new(
                None,
                Reason::AmbiguousKind,
                "24 hex digits are an ObjectId only if the leading four bytes are a time, and \
                 these are not; a truncated hash fits the same shape"
                    .to_string(),
            )
        });
    }
    Verdict::Named(Id::at(Kind::ObjectId, timestamp))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: Clock = Clock::at(1_786_492_800_000);

    fn named(token: &str) -> Id {
        match classify(token, CLOCK) {
            Verdict::Named(id) => id,
            other => panic!("{token} was {other:?}, not a named ObjectId"),
        }
    }

    fn refused(token: &str) -> Refusal {
        match classify(token, CLOCK) {
            Verdict::Refused(refusal) => refusal,
            other => panic!("{token} was {other:?}, not a refusal"),
        }
    }

    /// The leading four bytes are `0x6a7bb780`, built from a known
    /// second rather than taken from a document, so the decode is pinned
    /// exactly.
    #[test]
    fn an_objectid_is_named_with_its_timestamp() {
        assert_eq!(
            named("6a7bb780a1b2c3d4e5f60718").timestamp,
            Some("2026-08-12T00:00:00.000Z".to_string())
        );
    }

    #[test]
    fn an_objectid_reads_in_either_case() {
        assert_eq!(
            named("6A7BB780A1B2C3D4E5F60718").timestamp,
            named("6a7bb780a1b2c3d4e5f60718").timestamp
        );
    }

    /// The refusal this module is built around: the shape fits and the
    /// only evidence of the kind says it does not.
    #[test]
    fn a_hex_run_whose_leading_bytes_are_not_a_time_is_ambiguous() {
        // 0xffffffff is the year 2106; 0x00000001 is 1970.
        for token in ["ffffffffa1b2c3d4e5f60718", "00000001a1b2c3d4e5f60718"] {
            let refusal = refused(token);
            assert_eq!(refusal.reason, Reason::AmbiguousKind, "{token}");
            assert_eq!(refusal.kind, None, "no kind was named");
            assert!(refusal.timestamp.is_some(), "the decode is still carried");
        }
    }

    /// The first 24 characters of a real git commit hash. Its leading
    /// four bytes are `0xe83c5163`, which is 2093 — outside the window,
    /// so it is refused rather than named.
    #[test]
    fn a_truncated_hash_is_refused_rather_than_named() {
        assert_eq!(
            refused("e83c5163316f89bfbde7d9ab").reason,
            Reason::AmbiguousKind
        );
    }

    #[test]
    fn a_run_that_is_not_hex_is_not_an_objectid() {
        assert_eq!(
            classify("6a7bb780a1b2c3d4e5f6071g", CLOCK),
            Verdict::Ignored
        );
        assert_eq!(classify("6a7bb780-a1b2-c3d4-e5f6", CLOCK), Verdict::Ignored);
    }
}
