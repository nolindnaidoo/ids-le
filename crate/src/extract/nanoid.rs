//! NanoIDs: 21 characters of `A-Za-z0-9_-`, and nothing else.
//!
//! This is the thinnest kind here by a wide margin. A NanoID has no
//! structure at all — no version, no checksum, no timestamp, not even a
//! fixed alphabet, since the generator takes both size and alphabet as
//! arguments. Only the *defaults* are recognisable, and only barely: 21
//! characters of base64url.
//!
//! So two gates stand in front of it.
//!
//! The first keeps words out. A 21-character run that is all lowercase,
//! or carries no digit, is a slug or an identifier in someone's code,
//! and refusing it as an ambiguous NanoID would bury the report.
//!
//! The second is the evidence. `-` and `_` are the two characters that
//! separate base64url from base62, so a run carrying either is a NanoID
//! and a run of 21 base62 characters is refused as `ambiguous_kind` — it
//! is equally a short token, a truncated hash, or a base62 id from any
//! of a dozen libraries. Roughly half of real NanoIDs carry one of the
//! two, which means roughly half arrive as refusals unless the
//! document's key names the scheme. That is the honest split, and
//! SPEC.md states it rather than papering over it with a coin flip.

use super::policy::{Id, Kind, Reason, Refusal, Verdict, key_mentions};

const LENGTH: usize = 21;

pub(crate) fn classify(token: &str, key: Option<&str>) -> Verdict {
    if !is_random_looking(token) {
        return Verdict::Ignored;
    }
    let base64url = token.bytes().any(|byte| byte == b'-' || byte == b'_');
    if !base64url && !key_mentions(key, "nanoid") {
        return Verdict::Refused(Refusal::new(
            None,
            Reason::AmbiguousKind,
            "21 base62 characters are a NanoID, a short token and a truncated hash in equal \
             measure; only a `-` or `_`, or a key naming the scheme, separates them"
                .to_string(),
        ));
    }
    Verdict::Named(Id::of(Kind::NanoId))
}

/// The gate that keeps slugs, constants and ordinary identifiers out.
///
/// Mixed case *and* a digit. Each alone is common in hand-written names
/// — `SOME_CONSTANT_NAME_ER`, `retry-after-2-seconds` — and together
/// they are not.
fn is_random_looking(token: &str) -> bool {
    token.len() == LENGTH
        && token.bytes().any(|byte| byte.is_ascii_digit())
        && token.bytes().any(|byte| byte.is_ascii_uppercase())
        && token.bytes().any(|byte| byte.is_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The NanoID README's own example, which carries both a `_` and a
    /// `-`, and the same string with those two replaced by letters.
    const BASE64URL: &str = "V1StGXR8_Z5jdHi6B-myT";
    const BASE62: &str = "V1StGXR8xZ5jdHi6BxmyT";

    fn refused(token: &str, key: Option<&str>) -> Refusal {
        match classify(token, key) {
            Verdict::Refused(refusal) => refusal,
            other => panic!("{token} was {other:?}, not a refusal"),
        }
    }

    #[test]
    fn a_base64url_character_is_the_evidence() {
        assert_eq!(
            classify(BASE64URL, None),
            Verdict::Named(Id::of(Kind::NanoId))
        );
        let refusal = refused(BASE62, None);
        assert_eq!(refusal.reason, Reason::AmbiguousKind);
        assert_eq!(refusal.kind, None, "no kind was named");
    }

    /// Either character alone is enough — half of real NanoIDs carry
    /// only one of them.
    #[test]
    fn either_base64url_character_settles_it() {
        assert_eq!(
            classify("V1StGXR8_Z5jdHi6BxmyT", None),
            Verdict::Named(Id::of(Kind::NanoId))
        );
        assert_eq!(
            classify("V1StGXR8xZ5jdHi6B-myT", None),
            Verdict::Named(Id::of(Kind::NanoId))
        );
    }

    #[test]
    fn a_key_naming_the_scheme_settles_a_base62_run() {
        assert_eq!(
            classify(BASE62, Some("session.nanoId")),
            Verdict::Named(Id::of(Kind::NanoId))
        );
        assert_eq!(
            refused(BASE62, Some("session.token")).reason,
            Reason::AmbiguousKind
        );
    }

    #[test]
    fn a_nanoid_carries_no_version_and_no_timestamp() {
        let Verdict::Named(id) = classify(BASE64URL, None) else {
            panic!("a NanoID");
        };
        assert_eq!(id.version, None);
        assert_eq!(id.variant, None);
        assert_eq!(id.timestamp, None, "a NanoID has no clock in it");
    }

    /// The gate. Every one of these is a name somebody wrote, and a
    /// report full of refusals about them is a report nobody reads.
    #[test]
    fn hand_written_names_of_the_same_length_are_ignored() {
        for token in [
            "retry-after-2-seconds",
            "SOME_CONSTANT_NAME_ER",
            "abcdefghijklmnopqrstu",
            "handler_for_the_thing",
        ] {
            assert_eq!(classify(token, None), Verdict::Ignored, "{token}");
        }
    }

    #[test]
    fn the_wrong_length_is_not_a_nanoid() {
        assert_eq!(classify("V1StGXR8_Z5jdHi6B-my", None), Verdict::Ignored);
        assert_eq!(classify("V1StGXR8_Z5jdHi6B-myTx", None), Verdict::Ignored);
    }
}
