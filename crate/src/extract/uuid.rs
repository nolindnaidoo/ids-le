//! UUIDs, in the canonical hyphenated form, at every version.
//!
//! Four of the five refusal reasons are reachable from here, which is
//! why this is the longest kind module: a UUID is the only scheme that
//! carries a self-description — a version nibble and two variant bits —
//! and a self-description is a thing that can disagree with the bytes
//! around it.
//!
//! **Only the 36-character hyphenated form is a UUID here.** The
//! unhyphenated 32-character form is refused as `ambiguous_kind` in
//! `policy.rs`, because it is character-for-character an MD5 digest and
//! nothing in a document distinguishes them.
//!
//! The timestamp comes out of v1, v6 and v7, and the three read
//! completely different bit layouts of the same 128 bits. v1 and v6
//! count 100-nanosecond intervals from the Gregorian reform in 1582; v7
//! counts milliseconds from the Unix epoch and puts them in the first 48
//! bits, which is the whole reason v7 exists.

use super::policy::{Clock, Id, Kind, Reason, Refusal, Variant, Verdict};
use super::time::iso8601_utc;

/// Where the hyphens sit in `8-4-4-4-12`.
const HYPHENS: [usize; 4] = [8, 13, 18, 23];
/// The version nibble and the variant nibble, as indices into the
/// hyphen-stripped 32-character form.
const VERSION_NIBBLE: usize = 12;
const VARIANT_NIBBLE: usize = 16;

/// 100-nanosecond intervals between 1582-10-15 and 1970-01-01.
const GREGORIAN_OFFSET_TICKS: i64 = 122_192_928_000_000_000;
const TICKS_PER_MILLISECOND: i64 = 10_000;

/// How many consecutive zero nibbles make a "random" UUID's claim
/// doubtful.
///
/// Twelve zero nibbles are 48 zero bits. In a v4 that is one outcome in
/// 2^48 at any given offset — so seeing one is not a coincidence worth
/// entertaining, it is a hand-written placeholder or a field that was
/// never filled. Eight would fire on real UUIDs; sixteen would miss the
/// half-zeroed ones that are the common case.
const SUSPICIOUS_ZERO_RUN: usize = 12;

pub(crate) fn classify(token: &str, clock: Clock) -> Verdict {
    let Some(hex) = strip_hyphens(token) else {
        return Verdict::Ignored;
    };
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Verdict::Refused(Refusal::new(
            Some(Kind::Uuid),
            Reason::Malformed,
            "the 8-4-4-4-12 shape is right and at least one character is not a hex digit"
                .to_string(),
        ));
    }

    // Before anything is read out of them: both of these are structurally
    // perfect and neither identifies anything.
    if let Some(detail) = placeholder(&hex) {
        return Verdict::Refused(Refusal::new(
            Some(Kind::Uuid),
            Reason::NilOrMax,
            detail.to_string(),
        ));
    }

    let version = nibble(&hex, VERSION_NIBBLE);
    let variant = variant_of(nibble(&hex, VARIANT_NIBBLE));

    // The version nibble only means "version" under the RFC variant. In
    // any other layout those four bits belong to a different field, so
    // reporting a version from them would be inventing one.
    if variant != Variant::Rfc4122 {
        return Verdict::Refused(Refusal {
            variant: Some(variant),
            ..Refusal::new(
                Some(Kind::Uuid),
                Reason::Malformed,
                format!(
                    "the variant bits say {}, not RFC 4122, so the version nibble is not a version",
                    name_of(variant)
                ),
            )
        });
    }
    if !(1..=8).contains(&version) {
        return Verdict::Refused(Refusal {
            variant: Some(variant),
            ..Refusal::new(
                Some(Kind::Uuid),
                Reason::Malformed,
                format!("RFC 9562 defines versions 1 through 8; this one claims {version}"),
            )
        });
    }

    // Reported, never resolved: the claim and the doubt are both facts
    // and this crate has no way to tell which one is the mistake.
    if version == 4 && longest_zero_run(&hex) >= SUSPICIOUS_ZERO_RUN {
        return Verdict::Refused(Refusal {
            version: Some(version),
            variant: Some(variant),
            ..Refusal::new(
                Some(Kind::Uuid),
                Reason::VersionClaimMismatch,
                format!(
                    "the version nibble claims v4, which is 122 random bits, and {} of its \
                     nibbles in a row are zero",
                    longest_zero_run(&hex)
                ),
            )
        });
    }

    let Some(milliseconds) = timestamp_ms(&hex, version) else {
        return Verdict::Named(Id {
            version: Some(version),
            variant: Some(variant),
            ..Id::of(Kind::Uuid)
        });
    };
    let timestamp = iso8601_utc(milliseconds);
    if !clock.is_plausible(milliseconds) {
        return Verdict::Refused(Refusal {
            version: Some(version),
            variant: Some(variant),
            timestamp: Some(timestamp),
            ..Refusal::new(
                Some(Kind::Uuid),
                Reason::TimestampImplausible,
                format!("the v{version} time field decodes outside the plausible range"),
            )
        });
    }
    Verdict::Named(Id {
        version: Some(version),
        variant: Some(variant),
        ..Id::at(Kind::Uuid, timestamp)
    })
}

/// The 32 characters of a canonically hyphenated UUID, or nothing.
fn strip_hyphens(token: &str) -> Option<String> {
    let bytes = token.as_bytes();
    if bytes.len() != 36 {
        return None;
    }
    let mut hex = String::with_capacity(32);
    for (index, byte) in bytes.iter().enumerate() {
        if HYPHENS.contains(&index) {
            if *byte != b'-' {
                return None;
            }
            continue;
        }
        // `_` and `-` are candidate-run characters and are not UUID
        // characters, so a run carrying one elsewhere is not a
        // misspelled UUID — it is something else entirely.
        if !byte.is_ascii_alphanumeric() {
            return None;
        }
        hex.push(char::from(*byte));
    }
    Some(hex)
}

/// The nil UUID and the max UUID, both named in RFC 9562, both
/// structurally valid, and neither one an identifier.
fn placeholder(hex: &str) -> Option<&'static str> {
    if hex.bytes().all(|byte| byte == b'0') {
        return Some("the nil UUID: 128 zero bits, which RFC 9562 defines as naming nothing");
    }
    if hex.bytes().all(|byte| byte.eq_ignore_ascii_case(&b'f')) {
        return Some("the max UUID: 128 one bits, which RFC 9562 defines as naming nothing");
    }
    None
}

fn nibble(hex: &str, index: usize) -> i64 {
    i64::from(
        char::from(hex.as_bytes()[index])
            .to_digit(16)
            .expect("the caller checked every character is a hex digit"),
    )
}

fn variant_of(nibble: i64) -> Variant {
    match nibble {
        0..=7 => Variant::Ncs,
        8..=11 => Variant::Rfc4122,
        12..=13 => Variant::Microsoft,
        _ => Variant::Future,
    }
}

fn name_of(variant: Variant) -> &'static str {
    match variant {
        Variant::Ncs => "NCS",
        Variant::Rfc4122 => "RFC 4122",
        Variant::Microsoft => "Microsoft",
        Variant::Future => "reserved-future",
    }
}

/// The longest run of zero nibbles, with the two nibbles that are not
/// random masked out.
///
/// Masking matters: in `00000000-0000-4000-8000-000000000000` the
/// version and variant nibbles are the only non-zero characters, and
/// counting straight through them would answer 30 for a value whose
/// longest *random* zero run is 12.
fn longest_zero_run(hex: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for (index, byte) in hex.bytes().enumerate() {
        if index == VERSION_NIBBLE || index == VARIANT_NIBBLE || byte != b'0' {
            current = 0;
            continue;
        }
        current += 1;
        longest = longest.max(current);
    }
    longest
}

/// Unix milliseconds out of the versions that carry a clock.
///
/// v2 (DCE Security) carries a v1 clock with its low 32 bits replaced by
/// a POSIX uid, so its time field is truncated to roughly seven-minute
/// resolution and is not the same claim. It is deliberately not decoded.
fn timestamp_ms(hex: &str, version: i64) -> Option<i64> {
    match version {
        1 => Some(from_gregorian(
            (field(hex, 12, 16) & 0x0FFF) << 48 | field(hex, 8, 12) << 32 | field(hex, 0, 8),
        )),
        6 => Some(from_gregorian(
            field(hex, 0, 8) << 28 | field(hex, 8, 12) << 12 | (field(hex, 12, 16) & 0x0FFF),
        )),
        7 => Some(field(hex, 0, 12)),
        _ => None,
    }
}

fn field(hex: &str, from: usize, to: usize) -> i64 {
    i64::from_str_radix(&hex[from..to], 16).expect("the caller checked every character is hex")
}

/// 100-nanosecond intervals since 1582-10-15, in Unix milliseconds.
/// Euclidean because a zeroed time field lands four centuries before the
/// epoch, and a truncating division would put it on the wrong day.
fn from_gregorian(ticks: i64) -> i64 {
    (ticks - GREGORIAN_OFFSET_TICKS).div_euclid(TICKS_PER_MILLISECOND)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: Clock = Clock::at(1_786_492_800_000);

    fn named(token: &str) -> Id {
        match classify(token, CLOCK) {
            Verdict::Named(id) => id,
            other => panic!("{token} was {other:?}, not a named UUID"),
        }
    }

    fn refused(token: &str) -> Refusal {
        match classify(token, CLOCK) {
            Verdict::Refused(refusal) => refusal,
            other => panic!("{token} was {other:?}, not a refusal"),
        }
    }

    #[test]
    fn a_v4_is_named_with_its_version_and_variant() {
        let id = named("f47ac10b-58cc-4372-a567-0e02b2c3d479");
        assert_eq!(id.version, Some(4));
        assert_eq!(id.variant, Some(Variant::Rfc4122));
        assert_eq!(id.timestamp, None, "a v4 carries no clock");
    }

    /// The three versions that carry a clock, each reading a different
    /// bit layout of the same 128 bits.
    ///
    /// The v1, v6 and v7 tokens are RFC 9562's own examples, and the RFC
    /// states that all three name the same instant — 2022-02-22
    /// 14:22:22 GMT-05:00. Three different layouts landing on one
    /// timestamp is a check on the decoders that this implementation
    /// cannot fake.
    #[test]
    fn every_time_bearing_version_decodes_to_the_rfcs_own_instant() {
        let instant = Some("2022-02-22T19:22:22.000Z".to_string());
        let v1 = named("c232ab00-9414-11ec-b3c8-9e6bdeced846");
        assert_eq!(v1.version, Some(1));
        assert_eq!(v1.variant, Some(Variant::Rfc4122));
        assert_eq!(
            v1.timestamp, instant,
            "v1: Gregorian ticks, low field first"
        );

        let v6 = named("1ec9414c-232a-6b00-b3c8-9e6bdeced846");
        assert_eq!(v6.version, Some(6));
        assert_eq!(v6.timestamp, instant, "v6: the same ticks, reordered");

        let v7 = named("017f22e2-79d0-7cc3-98c4-dc0c0c07398f");
        assert_eq!(v7.version, Some(7));
        assert_eq!(
            v7.timestamp,
            Some("2022-02-22T19:22:22.032Z".to_string()),
            "v7: 48 bits of Unix milliseconds, and the RFC's example carries 32 more of them \
             than its v1 and v6 do"
        );
    }

    /// A v7 built for a known millisecond rather than taken from the
    /// RFC, so the corpus pins a decode this crate can be held to
    /// exactly.
    #[test]
    fn a_v7_decodes_the_millisecond_it_was_built_from() {
        assert_eq!(
            named("019ff344-cc00-7abc-8def-0123456789ab").timestamp,
            Some("2026-08-12T00:00:00.000Z".to_string())
        );
    }

    #[test]
    fn the_nil_and_max_uuids_are_refused_as_placeholders() {
        for token in [
            "00000000-0000-0000-0000-000000000000",
            "ffffffff-ffff-ffff-ffff-ffffffffffff",
            "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF",
        ] {
            let refusal = refused(token);
            assert_eq!(refusal.reason, Reason::NilOrMax, "{token}");
            assert_eq!(refusal.kind, Some(Kind::Uuid));
        }
    }

    /// The nil UUID is checked before the variant is read, because its
    /// variant bits are `0000` and it would otherwise be refused as an
    /// NCS UUID — a true statement that tells the reader nothing about
    /// what is actually wrong.
    #[test]
    fn the_nil_uuid_is_named_a_placeholder_rather_than_a_bad_variant() {
        assert_eq!(
            refused("00000000-0000-0000-0000-000000000000").reason,
            Reason::NilOrMax
        );
    }

    #[test]
    fn a_non_rfc_variant_is_malformed_and_reports_which_variant() {
        let refusal = refused("f47ac10b-58cc-4372-1567-0e02b2c3d479");
        assert_eq!(refusal.reason, Reason::Malformed);
        assert_eq!(refusal.variant, Some(Variant::Ncs));
        assert_eq!(refusal.version, None, "a version was not claimed");
        let microsoft = refused("f47ac10b-58cc-4372-c567-0e02b2c3d479");
        assert_eq!(microsoft.variant, Some(Variant::Microsoft));
        let future = refused("f47ac10b-58cc-4372-f567-0e02b2c3d479");
        assert_eq!(future.variant, Some(Variant::Future));
    }

    #[test]
    fn a_version_outside_the_rfc_is_malformed() {
        let refusal = refused("f47ac10b-58cc-9372-a567-0e02b2c3d479");
        assert_eq!(refusal.reason, Reason::Malformed);
        assert!(refusal.detail.contains('9'), "{}", refusal.detail);
    }

    #[test]
    fn a_non_hex_character_in_the_right_shape_is_malformed() {
        let refusal = refused("f47ac10b-58cc-4372-a567-0e02b2c3d47z");
        assert_eq!(refusal.reason, Reason::Malformed);
    }

    /// The claim and the doubt are both reported. Nothing here decides
    /// which one is the mistake.
    #[test]
    fn a_v4_that_is_obviously_not_random_is_a_version_claim_mismatch() {
        let refusal = refused("00000000-0000-4000-8000-000000000000");
        assert_eq!(refusal.reason, Reason::VersionClaimMismatch);
        assert_eq!(refusal.version, Some(4), "the claim is still reported");
        assert_eq!(refusal.variant, Some(Variant::Rfc4122));
    }

    /// The threshold has to miss real UUIDs or it is a false-positive
    /// generator, and this one has an eight-nibble zero run in it.
    #[test]
    fn an_ordinary_v4_with_a_short_zero_run_is_still_named() {
        assert_eq!(
            named("a1b2c3d4-0000-4000-8000-00005e6f7a8b").version,
            Some(4)
        );
    }

    /// The decode is reported next to the flag. A refusal that hid the
    /// timestamp would leave the reader nothing to judge.
    #[test]
    fn an_implausible_v7_reports_the_decode_and_the_flag() {
        let refusal = refused("7fffffff-ffff-7abc-8def-0123456789ab");
        assert_eq!(refusal.reason, Reason::TimestampImplausible);
        assert_eq!(refusal.version, Some(7));
        assert!(refusal.timestamp.is_some(), "the decode is carried");
    }

    #[test]
    fn a_run_that_is_not_hyphenated_like_a_uuid_is_not_one() {
        assert_eq!(
            classify("f47ac10b58cc4372a5670e02b2c3d479abcd", CLOCK),
            Verdict::Ignored
        );
        assert_eq!(
            classify("f47ac10b_58cc_4372_a567_0e02b2c3d479", CLOCK),
            Verdict::Ignored
        );
    }

    /// v2's time field is a v1 clock with its low 32 bits overwritten by
    /// a uid, so it resolves to roughly seven minutes and is a different
    /// claim. It is named, and it carries no timestamp.
    #[test]
    fn a_v2_is_named_without_a_timestamp() {
        let id = named("000004d2-92e8-21ed-8100-3fdb0085247e");
        assert_eq!(id.version, Some(2));
        assert_eq!(id.timestamp, None);
    }

    #[test]
    fn the_zero_run_masks_the_version_and_variant_nibbles() {
        assert_eq!(longest_zero_run("00000000000040008000000000000000"), 15);
        assert_eq!(longest_zero_run("ffffffffffff4fff8fffffffffffffff"), 0);
    }
}
