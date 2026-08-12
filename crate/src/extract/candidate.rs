//! Every run of text that could be an identifier, with where it starts.
//!
//! One scanner for every format, which is the structural difference
//! between this crate and `numbers-le`. There, a number's printed form
//! and its source text differ — `0x1A` is reported as `26` — so finding
//! a value's position needs a search by value and sometimes fails. An
//! identifier's raw text **is** its value, so the scan that finds it
//! already knows where it is. Nothing here is ever unlocated, and there
//! is no count for it.
//!
//! A candidate is a **maximal** run over `[0-9A-Za-z_-]`, and maximal is
//! the load-bearing word. `user_550e8400-e29b-41d4-a716-446655440000` is
//! one run of 45 characters, not a prefix beside a UUID — so it matches
//! no shape and is reported as nothing at all. Stripping a prefix this
//! tool cannot verify would be guessing which part of a string is the
//! identifier, and that is the one thing it does not do. SPEC.md states
//! the boundary.

/// The shortest and longest runs any kind can be: a Snowflake is 17
/// digits at its shortest, a hyphenated UUID is 36 characters. Filtering
/// here rather than in the classifier keeps ordinary prose from
/// allocating a candidate per word.
const SHORTEST: usize = 17;
const LONGEST: usize = 36;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Candidate<'a> {
    pub(crate) text: &'a str,
    pub(crate) start: usize,
}

/// Whether a byte can be part of an identifier run.
///
/// `-` and `_` are in because a UUID is hyphenated and NanoID's default
/// alphabet is base64url. Everything above ASCII is out: no identifier
/// scheme here uses it, and a run that stops at a non-ASCII byte is a
/// run that stops at the edge of a word in prose.
const fn is_run_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

pub(crate) fn candidates(text: &str) -> Vec<Candidate<'_>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;

    while at < bytes.len() {
        if !is_run_byte(bytes[at]) {
            at += 1;
            continue;
        }
        let start = at;
        while at < bytes.len() && is_run_byte(bytes[at]) {
            at += 1;
        }
        // The run is ASCII by construction, so byte length is character
        // length and the slice is always on a character boundary.
        if (SHORTEST..=LONGEST).contains(&(at - start)) {
            out.push(Candidate {
                text: &text[start..at],
                start,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(input: &str) -> Vec<&str> {
        candidates(input)
            .into_iter()
            .map(|candidate| candidate.text)
            .collect()
    }

    const UUID: &str = "550e8400-e29b-41d4-a716-446655440000";

    #[test]
    fn a_hyphenated_uuid_is_one_run() {
        assert_eq!(texts(UUID), [UUID]);
    }

    #[test]
    fn quotes_and_punctuation_bound_a_run() {
        assert_eq!(texts(&format!("{{\"id\":\"{UUID}\"}}")), [UUID]);
        assert_eq!(texts(&format!("the id {UUID}, then more")), [UUID]);
        assert_eq!(texts(&format!("urn:uuid:{UUID}")), [UUID]);
    }

    /// The boundary this module exists to hold. A prefix and an
    /// identifier are one run, not a prefix beside an identifier —
    /// deciding where the prefix ended would be guessing, so a prefixed
    /// UUID is longer than any kind and yields nothing at all.
    #[test]
    fn a_prefixed_identifier_is_one_run_and_matches_nothing() {
        assert!(
            texts(&format!("user_{UUID}")).is_empty(),
            "41 characters is longer than any kind"
        );
        // Short enough to be a candidate, and no kind claims it: the run
        // reaching the classifier is the whole `x-` prefix included.
        assert_eq!(
            texts("x-6a7bb780a1b2c3d4e5f60718"),
            ["x-6a7bb780a1b2c3d4e5f60718"]
        );
    }

    #[test]
    fn the_offset_points_at_the_start_of_the_run() {
        let input = format!("id = {UUID}");
        let found = candidates(&input);
        assert_eq!(found[0].start, 5);
        assert_eq!(&input[found[0].start..], UUID);
    }

    #[test]
    fn runs_come_back_in_document_order() {
        let input = format!("{UUID}\n01KZSM9K00ABCDEFGH12345678\n");
        assert_eq!(texts(&input), [UUID, "01KZSM9K00ABCDEFGH12345678"]);
    }

    /// Prose is almost entirely short words, and a scan that allocated
    /// one candidate each would spend its whole time on runs no kind can
    /// be.
    #[test]
    fn runs_outside_every_kinds_length_are_not_candidates() {
        assert!(texts("the quick brown fox jumped over").is_empty());
        assert!(texts(&"a".repeat(LONGEST + 1)).is_empty());
        assert_eq!(texts(&"a".repeat(LONGEST)).len(), 1);
        assert_eq!(texts(&"a".repeat(SHORTEST)).len(), 1);
        assert!(texts(&"a".repeat(SHORTEST - 1)).is_empty());
    }

    /// A non-ASCII byte ends a run. No identifier scheme here uses one,
    /// and continuing through it would splice two words together.
    #[test]
    fn a_non_ascii_byte_ends_a_run() {
        let input = format!("café{UUID}");
        assert_eq!(texts(&input), [UUID]);
    }

    #[test]
    fn an_empty_document_yields_nothing() {
        assert!(texts("").is_empty());
    }
}
