//! Snowflakes: a 64-bit integer whose top 41 bits are milliseconds
//! since a platform-chosen epoch.
//!
//! **A bare integer is not a finding.** This is the sharpest boundary in
//! the crate and the one most worth stating: a Snowflake looks exactly
//! like a large number, and a document is full of large numbers — a
//! timestamp in microseconds, a byte count, a phone number, an account
//! number. There is nothing *in* an 18-digit integer that says it is an
//! identifier, so nothing here treats one as an identifier on its own.
//!
//! Two pieces of evidence are required together:
//!
//! - **Digit count in range.** 17 to 19 digits, which is where the
//!   generators have been since roughly 2015 and where they stay until
//!   the field overflows.
//! - **A key that names an id.** The field's own name — the leaf of the
//!   key path — ends in `id` or mentions `snowflake`.
//!
//! In a document with no keys at all, which is every plain-text file,
//! the second is unobtainable and no integer is ever a Snowflake. That
//! is deliberate. SPEC.md says so.
//!
//! The epoch is the second refusal. Twitter's is 2010-11-04 and
//! Discord's is 2015-01-01, four years apart, and both produce a
//! plausible instant for any real identifier — so where nothing names
//! the platform, both fit and neither is chosen.

use super::policy::{
    Clock, Id, Kind, Reason, Refusal, Verdict, flatten_key, key_mentions, leaf_key,
};
use super::time::iso8601_utc;

/// 2010-11-04T01:42:54.657Z — the instant Twitter's first Snowflake
/// generator was given.
const TWITTER_EPOCH_MS: i64 = 1_288_834_974_657;
/// 2015-01-01T00:00:00.000Z.
const DISCORD_EPOCH_MS: i64 = 1_420_070_400_000;
/// 22 bits of worker id and sequence sit below the timestamp.
const TIMESTAMP_SHIFT: u32 = 22;

/// Words that name a platform, and so name an epoch. Each list is the
/// vocabulary that platform's own API uses for its identifier fields.
const DISCORD_WORDS: [&str; 4] = ["discord", "guild", "channel", "message"];
const TWITTER_WORDS: [&str; 3] = ["twitter", "tweet", "status"];

pub(crate) fn classify(token: &str, key: Option<&str>, clock: Clock) -> Verdict {
    if !names_an_id(key) {
        return Verdict::Ignored;
    }
    let Ok(value) = token.parse::<u64>() else {
        return Verdict::Ignored;
    };
    let shifted = i64::try_from(value >> TIMESTAMP_SHIFT).expect("42 bits fit i64");

    let discord = shifted + DISCORD_EPOCH_MS;
    let twitter = shifted + TWITTER_EPOCH_MS;

    let Some(milliseconds) = chosen_epoch(key, discord, twitter, clock) else {
        return Verdict::Refused(Refusal {
            timestamp: Some(iso8601_utc(twitter)),
            ..Refusal::new(
                Some(Kind::Snowflake),
                Reason::AmbiguousKind,
                format!(
                    "the Twitter epoch decodes this to {} and the Discord epoch to {}; both are \
                     plausible and no key here names the platform",
                    iso8601_utc(twitter),
                    iso8601_utc(discord)
                ),
            )
        });
    };

    let timestamp = iso8601_utc(milliseconds);
    if !clock.is_plausible(milliseconds) {
        return Verdict::Refused(Refusal {
            timestamp: Some(timestamp),
            ..Refusal::new(
                Some(Kind::Snowflake),
                Reason::TimestampImplausible,
                "the top 42 bits decode outside the plausible range under either epoch".to_string(),
            )
        });
    }
    Verdict::Named(Id::at(Kind::Snowflake, timestamp))
}

/// Whether the field's own name says it holds an identifier.
///
/// The *leaf* of the key path, not the whole path: `snowflakes.count` is
/// a count that happens to sit under a table about identifiers, and
/// matching the whole path would name it one.
fn names_an_id(key: Option<&str>) -> bool {
    let Some(path) = key else {
        return false;
    };
    let leaf = flatten_key(leaf_key(path));
    leaf.ends_with("id") || leaf.contains("snowflake")
}

/// The epoch the document chose, or nothing when it chose neither and
/// both fit.
fn chosen_epoch(key: Option<&str>, discord: i64, twitter: i64, clock: Clock) -> Option<i64> {
    let says_discord = DISCORD_WORDS.iter().any(|word| key_mentions(key, word));
    let says_twitter = TWITTER_WORDS.iter().any(|word| key_mentions(key, word));
    match (says_discord, says_twitter) {
        (true, false) => return Some(discord),
        (false, true) => return Some(twitter),
        // A key naming both platforms is a key naming neither.
        _ => {}
    }

    // Nothing named the platform, so the clock is the last thing left to
    // ask. Where exactly one epoch produces a plausible instant, that is
    // an answer; where both do — which is the ordinary case for a real
    // identifier — it is not.
    match (clock.is_plausible(discord), clock.is_plausible(twitter)) {
        (true, true) => None,
        (true, false) => Some(discord),
        // Twitter's epoch answers the remaining two cases: it is the one
        // that fits, or neither fits and the refusal is about the time
        // rather than the epoch — where the caller still needs a decode
        // to report, and Twitter's is the original scheme.
        (false, _) => Some(twitter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: Clock = Clock::at(1_786_492_800_000);
    /// Built so that the Discord epoch decodes it to 2026-08-12.
    const DISCORD_ID: &str = "1536886938009600000";

    fn named(token: &str, key: &str) -> Id {
        match classify(token, Some(key), CLOCK) {
            Verdict::Named(id) => id,
            other => panic!("{token} was {other:?}, not a named Snowflake"),
        }
    }

    fn refused(token: &str, key: &str) -> Refusal {
        match classify(token, Some(key), CLOCK) {
            Verdict::Refused(refusal) => refusal,
            other => panic!("{token} was {other:?}, not a refusal"),
        }
    }

    /// The boundary this module is built around, stated as a test so it
    /// cannot be relaxed by accident.
    #[test]
    fn a_bare_integer_with_no_key_is_never_a_snowflake() {
        assert_eq!(classify(DISCORD_ID, None, CLOCK), Verdict::Ignored);
    }

    #[test]
    fn an_integer_under_a_key_that_names_no_id_is_ignored() {
        for key in ["bytes_written", "population", "account.balance"] {
            assert_eq!(
                classify(DISCORD_ID, Some(key), CLOCK),
                Verdict::Ignored,
                "{key}"
            );
        }
    }

    #[test]
    fn a_key_ending_in_id_is_the_evidence() {
        for key in ["id", "user_id", "userId", "USER-ID", "snowflake"] {
            assert_ne!(
                classify(DISCORD_ID, Some(key), CLOCK),
                Verdict::Ignored,
                "{key}"
            );
        }
    }

    /// The leaf, not the whole path: a count under a table about
    /// identifiers is still a count.
    #[test]
    fn only_the_leaf_of_the_key_path_is_read_for_id_evidence() {
        assert_eq!(
            classify(DISCORD_ID, Some("snowflakes.count"), CLOCK),
            Verdict::Ignored
        );
        assert_ne!(
            classify(DISCORD_ID, Some("snowflakes.channel_id"), CLOCK),
            Verdict::Ignored
        );
    }

    #[test]
    fn a_key_naming_the_platform_chooses_its_epoch() {
        assert_eq!(
            named(DISCORD_ID, "channel_id").timestamp,
            Some("2026-08-12T00:00:00.000Z".to_string())
        );
        assert_eq!(
            named("2087328207467446272", "tweet_id").timestamp,
            Some("2026-08-12T00:00:00.000Z".to_string())
        );
    }

    #[test]
    fn every_platform_word_selects_its_epoch() {
        for key in ["discord_id", "guild_id", "channel_id", "message_id"] {
            assert_eq!(
                named(DISCORD_ID, key).timestamp,
                Some("2026-08-12T00:00:00.000Z".to_string()),
                "{key}"
            );
        }
        for key in ["twitter_id", "tweet_id", "status_id"] {
            assert_eq!(
                named("2087328207467446272", key).timestamp,
                Some("2026-08-12T00:00:00.000Z".to_string()),
                "{key}"
            );
        }
    }

    /// The second refusal: both epochs fit and the document does not
    /// choose, so neither does this.
    #[test]
    fn an_unnamed_platform_leaves_two_epochs_fitting_and_refuses() {
        let refusal = refused(DISCORD_ID, "user_id");
        assert_eq!(refusal.reason, Reason::AmbiguousKind);
        assert_eq!(refusal.kind, Some(Kind::Snowflake));
        assert!(refusal.detail.contains("Twitter"), "{}", refusal.detail);
        assert!(refusal.detail.contains("Discord"), "{}", refusal.detail);
    }

    /// A key naming both platforms names neither.
    #[test]
    fn a_key_naming_two_platforms_is_still_ambiguous() {
        assert_eq!(
            refused(DISCORD_ID, "discord_tweet_id").reason,
            Reason::AmbiguousKind
        );
    }

    /// The far end of the field. A 19-digit value at the top of the
    /// range decodes to the 2080s under either epoch, so the flag fires
    /// and the decode still comes with it.
    #[test]
    fn a_value_too_large_for_the_field_is_an_implausible_timestamp() {
        let refusal = refused("9223372036854775807", "message_id");
        assert_eq!(refusal.reason, Reason::TimestampImplausible);
        assert!(refusal.timestamp.is_some(), "the decode is carried");
    }

    /// Seventeen digits under the Discord epoch land in 2015–2016, which
    /// is real. The router's length window is not decoration.
    #[test]
    fn a_seventeen_digit_identifier_still_decodes() {
        assert_eq!(
            named("81384788765712384", "guild_id").timestamp,
            Some("2015-08-13T13:54:05.698Z".to_string())
        );
    }
}
