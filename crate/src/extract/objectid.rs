//! MongoDB ObjectIds: twelve bytes as 24 hex digits, the first four of
//! them a Unix second.
//!
//! **An ObjectId has nothing to validate.** A UUID carries a version
//! nibble and two variant bits; a ULID is restricted to Crockford base32
//! and its leading character cannot exceed a 48-bit clock; a NanoID has
//! an alphabet. This crate checks every one of those. An ObjectId's
//! entire specification is *24 hex characters* — so every 24-hex run is
//! a structurally perfect ObjectId, and always will be. A truncated
//! SHA-1, the front of a git object name, a hex dump of anything at all:
//! the run cannot tell them apart because there is nothing in it to
//! tell them apart with.
//!
//! So **context is the only evidence that exists, and this module
//! requires it**: a 24-hex run is named an ObjectId when the document's
//! own name for the field says it holds an identifier — the leaf of the
//! key path ends in `id` — *and* its leading four bytes decode to a
//! plausible time. Either signal missing is `ambiguous_kind`.
//!
//! That is not a new rule. It is the rule `snowflake.rs` already
//! applies, on the same reasoning: a bare integer is not a Snowflake
//! without a key naming an id, because nothing *in* an 18-digit integer
//! says it is an identifier. An ObjectId is the same class of run, and
//! the two share one predicate — `policy::names_an_id` — rather than two
//! that can drift.
//!
//! The trade is stated rather than hidden. A genuine ObjectId sitting in
//! prose, or under a key like `checksum`, now comes back refused: this
//! buys precision with recall. What was bought: on 600 abbreviated SHA-1
//! and MD5 digests in `fixtures/documents/hashes.txt` the old rule named
//! 163 of them, 27.2%; the new one names none. What remains is stated
//! too — under a key that *does* name an id, the timestamp is the only
//! remaining filter and it admits roughly a quarter of random hex, so a
//! digest stored in a field called `commitId` is still named.
//! `tests/coverage_matrix.rs` measures both numbers on every run.
//!
//! The decoded timestamp rides on the refusal as well as on the finding,
//! because a reader looking at a "2003-11-08" ObjectId in a document
//! written last week has what they need to disagree either way.

use super::policy::{Clock, Id, Kind, Reason, Refusal, Verdict, is_hex, names_an_id};
use super::time::iso8601_utc;

const LENGTH: usize = 24;
/// The four bytes of Unix seconds at the front.
const TIME_CHARS: usize = 8;

pub(crate) fn classify(token: &str, key: Option<&str>, clock: Clock) -> Verdict {
    if token.len() != LENGTH || !is_hex(token) {
        return Verdict::Ignored;
    }

    let seconds =
        i64::from_str_radix(&token[..TIME_CHARS], 16).expect("eight checked hex digits fit i64");
    let milliseconds = seconds * 1_000;
    let timestamp = iso8601_utc(milliseconds);

    // The kind is asked for first, and refused first, because it is the
    // question: an ObjectId with no context is a shape, and a shape that
    // every hash in the tree also has.
    let refusal = |detail: &str| {
        Verdict::Refused(Refusal {
            timestamp: Some(timestamp.clone()),
            ..Refusal::new(None, Reason::AmbiguousKind, detail.to_string())
        })
    };

    if !names_an_id(key) {
        return refusal(
            "24 hex digits are an ObjectId, a truncated hash and a hex dump in equal measure, \
             and nothing in this document names this field an identifier",
        );
    }
    if !clock.is_plausible(milliseconds) {
        return refusal(
            "the field is named like an identifier, and 24 hex digits are an ObjectId only if \
             the leading four bytes are also a time; these are not, and a truncated hash fits \
             the same shape",
        );
    }
    Verdict::Named(Id::at(Kind::ObjectId, timestamp))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: Clock = Clock::at(1_786_492_800_000);
    /// Leading four bytes `0x6a7bb780`, built from a known second rather
    /// than taken from a document, so the decode is pinned exactly.
    const OBJECT_ID: &str = "6a7bb780a1b2c3d4e5f60718";

    fn named(token: &str, key: &str) -> Id {
        match classify(token, Some(key), CLOCK) {
            Verdict::Named(id) => id,
            other => panic!("{token} was {other:?}, not a named ObjectId"),
        }
    }

    fn refused(token: &str, key: Option<&str>) -> Refusal {
        match classify(token, key, CLOCK) {
            Verdict::Refused(refusal) => refusal,
            other => panic!("{token} was {other:?}, not a refusal"),
        }
    }

    #[test]
    fn an_objectid_under_a_key_naming_an_id_is_named_with_its_timestamp() {
        assert_eq!(
            named(OBJECT_ID, "_id").timestamp,
            Some("2026-08-12T00:00:00.000Z".to_string())
        );
    }

    /// Every spelling a document reaches for. The predicate is shared
    /// with `snowflake.rs`, and this is the half of it this kind meets.
    #[test]
    fn every_way_a_document_names_an_id_field_is_evidence() {
        for key in [
            "_id",
            "id",
            "objectId",
            "userId",
            "user_id",
            "documents.[0]._id",
            "record.$oid",
        ] {
            assert_eq!(
                named(OBJECT_ID, key).kind,
                Kind::ObjectId,
                "{key} did not settle it"
            );
        }
    }

    #[test]
    fn an_objectid_reads_in_either_case() {
        assert_eq!(
            named("6A7BB780A1B2C3D4E5F60718", "_id").timestamp,
            named(OBJECT_ID, "_id").timestamp
        );
    }

    /// **The rule this module is built around.** The same 24 characters,
    /// named where the document says the field holds an identifier and
    /// refused where it does not — because there is nothing in the run
    /// itself to tell an ObjectId from a truncated hash, and a shape is
    /// not evidence.
    #[test]
    fn the_same_run_is_named_under_an_id_key_and_refused_without_one() {
        assert_eq!(named(OBJECT_ID, "_id").kind, Kind::ObjectId);
        for key in [Some("checksum"), Some("commit"), Some("digest"), None] {
            let refusal = refused(OBJECT_ID, key);
            assert_eq!(refusal.reason, Reason::AmbiguousKind, "{key:?}");
            assert_eq!(refusal.kind, None, "{key:?}: a kind was named");
            assert_eq!(
                refusal.timestamp,
                Some("2026-08-12T00:00:00.000Z".to_string()),
                "{key:?}: the decode is carried into the refusal"
            );
            assert!(
                refusal.detail.contains("names this field an identifier"),
                "{key:?}: {}",
                refusal.detail
            );
        }
    }

    /// A document with no keys at all — every plain-text file — names no
    /// field anything, so no run in one is ever an ObjectId. The same
    /// sentence is true of `snowflake.rs`, deliberately.
    #[test]
    fn a_run_in_a_document_with_no_keys_is_never_an_objectid() {
        assert_eq!(refused(OBJECT_ID, None).reason, Reason::AmbiguousKind);
    }

    /// Both signals are required. A key naming an identifier does not
    /// rescue a decode that is not a time.
    #[test]
    fn a_hex_run_whose_leading_bytes_are_not_a_time_is_ambiguous() {
        // 0xffffffff is the year 2106; 0x00000001 is 1970.
        for token in ["ffffffffa1b2c3d4e5f60718", "00000001a1b2c3d4e5f60718"] {
            let refusal = refused(token, Some("_id"));
            assert_eq!(refusal.reason, Reason::AmbiguousKind, "{token}");
            assert_eq!(refusal.kind, None, "no kind was named");
            assert!(refusal.timestamp.is_some(), "the decode is still carried");
            assert!(
                refusal.detail.contains("also a time"),
                "the refusal does not say which signal was missing: {}",
                refusal.detail
            );
        }
    }

    /// The first 24 characters of a real git commit hash, under a key
    /// that names an id — which is exactly where one ends up, in a field
    /// called `commitId`. Its leading four bytes are `0xe83c5163`, which
    /// is 2093, so the timestamp still refuses it.
    #[test]
    fn a_truncated_hash_is_refused_rather_than_named() {
        assert_eq!(
            refused("e83c5163316f89bfbde7d9ab", Some("commitId")).reason,
            Reason::AmbiguousKind
        );
    }

    #[test]
    fn a_run_that_is_not_hex_is_not_an_objectid() {
        assert_eq!(
            classify("6a7bb780a1b2c3d4e5f6071g", Some("_id"), CLOCK),
            Verdict::Ignored
        );
        assert_eq!(
            classify("6a7bb780-a1b2-c3d4-e5f6", Some("_id"), CLOCK),
            Verdict::Ignored
        );
    }
}
