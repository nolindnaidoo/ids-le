//! The one classification policy every kind shares, and the router that
//! sends a candidate to its kind.
//!
//! **Refusing is the product.** Anything else in this crate could be
//! rebuilt from a regex and a shift; deciding honestly that a run of
//! characters *cannot* be named is what a regex cannot do. So a refusal
//! is a row like any other — it carries the raw text, its position, a
//! named reason and, where one was decoded, the timestamp that made the
//! reason true. It is never a dropped row, and never a silent success.
//!
//! Routing is by length, because the five kinds barely overlap in it:
//! 36 hyphenated is a UUID, 26 is a ULID, 24 hex is an ObjectId, 21 is a
//! NanoID, 17–19 digits is a Snowflake, and 32 hex is the one genuine
//! collision — an unhyphenated UUID and an MD5 digest are the same 32
//! characters, so nothing here picks between them.

use serde::{Deserialize, Serialize};

use super::{nanoid, objectid, snowflake, ulid, uuid};

/// The identifier schemes this crate can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    Uuid,
    Ulid,
    NanoId,
    ObjectId,
    Snowflake,
}

/// The names a caller may use for a kind, for `--kind` and the tool
/// schema's enum. Held equal to `Kind` by a test.
pub(crate) const KIND_NAMES: [&str; 5] = ["uuid", "ulid", "nanoid", "objectid", "snowflake"];

impl Kind {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_lowercase().as_str() {
            "uuid" => Some(Self::Uuid),
            "ulid" => Some(Self::Ulid),
            "nanoid" => Some(Self::NanoId),
            "objectid" => Some(Self::ObjectId),
            "snowflake" => Some(Self::Snowflake),
            _ => None,
        }
    }
}

/// A UUID's two variant bits, reported because they say which
/// specification the rest of the layout follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Variant {
    /// `0xxx` — Apollo NCS, predating RFC 4122.
    Ncs,
    /// `10xx` — RFC 4122, since superseded by RFC 9562. The only variant
    /// whose version nibble means what this crate reports.
    Rfc4122,
    /// `110x` — Microsoft's COM/DCOM layout.
    Microsoft,
    /// `111x` — reserved for future definition.
    Future,
}

/// Why a candidate was refused. Every one of these is a *named* answer,
/// not an absence of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Reason {
    /// Two or more schemes fit and nothing in the document chooses
    /// between them.
    AmbiguousKind,
    /// The right shape, and it failed validation.
    Malformed,
    /// The nil or max UUID. Structurally valid, and almost always a
    /// placeholder that escaped.
    NilOrMax,
    /// The version nibble claims one thing and the bytes suggest
    /// another. Both are reported; neither is resolved.
    VersionClaimMismatch,
    /// A timestamp decoded cleanly and landed somewhere it should not
    /// be. The decode is reported alongside the flag.
    TimestampImplausible,
}

/// What one candidate turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Not identifier-shaped. Not a row — an audit of a repository would
    /// otherwise be mostly rows about ordinary words.
    Ignored,
    Named(Id),
    Refused(Refusal),
}

/// An identifier this crate is willing to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Id {
    pub(crate) kind: Kind,
    pub(crate) version: Option<i64>,
    pub(crate) variant: Option<Variant>,
    pub(crate) timestamp: Option<String>,
}

impl Id {
    /// A kind with nothing to decode out of it.
    pub(crate) const fn of(kind: Kind) -> Self {
        Self {
            kind,
            version: None,
            variant: None,
            timestamp: None,
        }
    }

    pub(crate) fn at(kind: Kind, timestamp: String) -> Self {
        Self {
            timestamp: Some(timestamp),
            ..Self::of(kind)
        }
    }
}

/// A candidate this crate will not name, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    /// The kind the shape claims, where the shape claims one. `None`
    /// when the refusal *is* that no kind could be named.
    pub(crate) kind: Option<Kind>,
    pub(crate) reason: Reason,
    pub(crate) detail: String,
    pub(crate) version: Option<i64>,
    pub(crate) variant: Option<Variant>,
    /// Reported even here: a refusal that hides the decode which caused
    /// it hands the reader a verdict with no evidence under it.
    pub(crate) timestamp: Option<String>,
}

impl Refusal {
    pub(crate) fn new(kind: Option<Kind>, reason: Reason, detail: String) -> Self {
        Self {
            kind,
            reason,
            detail,
            version: None,
            variant: None,
            timestamp: None,
        }
    }
}

/// 1990-01-01T00:00:00Z. Nothing this tool will meet was minted before
/// it: UUID v1 is the oldest scheme here and saw no use at that scale
/// until well after, and a decode landing in the 1970s is a field read
/// at the wrong offset rather than an old record.
pub(crate) const EARLIEST_PLAUSIBLE_MS: i64 = 631_152_000_000;

/// How far past now a timestamp may sit before it is suspect. A year is
/// wide enough for a clock-skewed generator and a deliberately
/// future-dated record, and narrow enough that a misread field lands
/// outside it.
const FUTURE_ALLOWANCE_MS: i64 = 365 * 86_400_000;

/// Now, passed in rather than read.
///
/// `extract/` has no clock for the same reason it has no filesystem: a
/// plausibility window that moves with the wall clock would make every
/// test that touches it fail on a future Tuesday. The surfaces read the
/// clock; this layer is handed the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Clock {
    pub(crate) now_ms: i64,
}

impl Clock {
    pub(crate) const fn at(now_ms: i64) -> Self {
        Self { now_ms }
    }

    pub(crate) fn is_plausible(self, milliseconds: i64) -> bool {
        (EARLIEST_PLAUSIBLE_MS..=self.now_ms + FUTURE_ALLOWANCE_MS).contains(&milliseconds)
    }
}

/// Name one candidate, or refuse it, or leave it alone.
///
/// `key` is the document's own word for where the candidate sits — the
/// dotted key path from `locate.rs`. It is evidence, not decoration:
/// three of the five kinds have a shape that a random string reaches by
/// accident, and the key name is the only thing in the document that
/// can settle it.
pub(crate) fn classify(token: &str, key: Option<&str>, clock: Clock) -> Verdict {
    match token.len() {
        36 => uuid::classify(token, clock),
        26 => ulid::classify(token, key, clock),
        24 => objectid::classify(token, clock),
        21 => nanoid::classify(token, key),
        17..=19 if token.bytes().all(|byte| byte.is_ascii_digit()) => {
            snowflake::classify(token, key, clock)
        }
        32 if is_hex(token) => Verdict::Refused(Refusal::new(
            None,
            Reason::AmbiguousKind,
            "32 hex digits are an unhyphenated UUID and an MD5 digest in equal measure; \
             nothing in this document chooses between them"
                .to_string(),
        )),
        length if is_near_miss_hex(token, length) => Verdict::Refused(Refusal::new(
            None,
            Reason::Malformed,
            format!(
                "{length} hex digits is not a whole number of bytes and is one digit from \
                 a hex identifier this tool knows"
            ),
        )),
        _ => Verdict::Ignored,
    }
}

pub(crate) fn is_hex(token: &str) -> bool {
    token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A hex run one digit either side of 24 (ObjectId) or 32 (unhyphenated
/// UUID / MD5).
///
/// The `a`–`f` requirement is what keeps a Snowflake out of here: 19
/// digits are 19 hex digits too, and a bare integer must reach
/// `snowflake.rs` rather than be refused as a truncated ObjectId.
fn is_near_miss_hex(token: &str, length: usize) -> bool {
    let near = length.abs_diff(24) == 1 || length.abs_diff(32) == 1;
    near && is_hex(token) && token.bytes().any(|byte| byte.is_ascii_alphabetic())
}

/// Whether the document's key path mentions a word.
///
/// Compared with separators removed, so `user_id`, `userId` and
/// `user-id` are the same evidence — which is the only way this is
/// usable across six formats whose conventions disagree.
pub(crate) fn key_mentions(key: Option<&str>, word: &str) -> bool {
    key.is_some_and(|path| flatten_key(path).contains(word))
}

/// The document's key path, lowercased with separators removed.
pub(crate) fn flatten_key(key: &str) -> String {
    key.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

/// The last segment of a dotted key path — the field's own name, rather
/// than the table or object it happens to sit in.
pub(crate) fn leaf_key(key: &str) -> &str {
    key.rsplit('.').next().unwrap_or(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: Clock = Clock::at(1_786_492_800_000);

    fn refusal(token: &str) -> Refusal {
        match classify(token, None, CLOCK) {
            Verdict::Refused(refusal) => refusal,
            other => panic!("{token} was {other:?}, not a refusal"),
        }
    }

    #[test]
    fn every_kind_name_round_trips_and_the_list_is_complete() {
        for name in KIND_NAMES {
            let kind = Kind::from_name(name).expect(name);
            assert_eq!(serde_json::to_value(kind).expect("serializes"), name);
        }
        for kind in [
            Kind::Uuid,
            Kind::Ulid,
            Kind::NanoId,
            Kind::ObjectId,
            Kind::Snowflake,
        ] {
            let name = serde_json::to_value(kind).expect("serializes");
            assert!(
                KIND_NAMES.contains(&name.as_str().expect("a string")),
                "{kind:?} is a kind but is not offered by name"
            );
        }
    }

    #[test]
    fn an_unknown_kind_name_is_refused() {
        assert_eq!(Kind::from_name("guid"), None);
        assert_eq!(Kind::from_name(""), None);
    }

    #[test]
    fn a_kind_name_is_normalised() {
        assert_eq!(Kind::from_name("  UUID "), Some(Kind::Uuid));
    }

    /// The one genuine cross-kind collision, and the refusal the whole
    /// design exists for: two schemes fit exactly, so neither is chosen.
    #[test]
    fn thirty_two_hex_digits_are_ambiguous_rather_than_a_uuid() {
        let refusal = refusal("5d41402abc4b2a76b9719d911017c592");
        assert_eq!(refusal.reason, Reason::AmbiguousKind);
        assert_eq!(refusal.kind, None);
        assert!(refusal.detail.contains("MD5"), "{}", refusal.detail);
    }

    #[test]
    fn a_hex_run_one_digit_from_an_identifier_length_is_malformed() {
        for token in [
            "6a7bb780a1b2c3d4e5f6071",           // 23
            "6a7bb780a1b2c3d4e5f607181",         // 25
            "5d41402abc4b2a76b9719d911017c59",   // 31
            "5d41402abc4b2a76b9719d911017c5921", // 33
        ] {
            let refusal = refusal(token);
            assert_eq!(refusal.reason, Reason::Malformed, "{token}");
        }
    }

    /// The reason the near-miss rule asks for a letter. Nineteen digits
    /// are nineteen hex digits, and a Snowflake must not be refused as a
    /// truncated ObjectId before it reaches its own module.
    #[test]
    fn a_run_of_digits_is_never_a_near_miss_hex_identifier() {
        assert!(!is_near_miss_hex("1234567890123456789", 19));
        assert!(!is_near_miss_hex("12345678901234567890123", 23));
    }

    #[test]
    fn an_ordinary_word_is_ignored_rather_than_refused() {
        assert_eq!(
            classify("configuration_value", None, CLOCK),
            Verdict::Ignored
        );
    }

    #[test]
    fn the_plausibility_window_opens_in_1990_and_closes_a_year_out() {
        assert!(CLOCK.is_plausible(EARLIEST_PLAUSIBLE_MS));
        assert!(!CLOCK.is_plausible(EARLIEST_PLAUSIBLE_MS - 1));
        assert!(CLOCK.is_plausible(CLOCK.now_ms + FUTURE_ALLOWANCE_MS));
        assert!(!CLOCK.is_plausible(CLOCK.now_ms + FUTURE_ALLOWANCE_MS + 1));
    }

    /// Six formats spell the same field six ways. Evidence that only
    /// matched one of them would be evidence in JSON and nowhere else.
    #[test]
    fn key_evidence_ignores_case_and_separators() {
        assert!(key_mentions(Some("user_id"), "userid"));
        assert!(key_mentions(Some("userId"), "userid"));
        assert!(key_mentions(Some("USER-ID"), "userid"));
        assert!(key_mentions(Some("orders.[0].userId"), "userid"));
        assert!(!key_mentions(Some("username"), "userid"));
        assert!(!key_mentions(None, "userid"));
    }

    #[test]
    fn the_leaf_of_a_key_path_is_the_fields_own_name() {
        assert_eq!(leaf_key("service.database.id"), "id");
        assert_eq!(leaf_key("id"), "id");
    }
}
