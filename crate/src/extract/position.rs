//! Byte offset → line/column, where **column is counted in UTF-16 code
//! units**.
//!
//! That is not an accident inherited from JavaScript. An editor reports
//! UTF-16 columns, so a person comparing this tool's output against the
//! file open in front of them needs the same number. Counting bytes
//! answers 6 where the correct answer is 5 on a line holding `café`, and
//! counting Unicode scalars answers 5 there but disagrees again on
//! anything astral.
//!
//! Lines and columns are 1-based.
//!
//! Each crate in this family stands on its own: no shared crate, no
//! published core, and nothing holding this file equal to the similar
//! ones in the sibling repos. Where they agree it is because the same
//! answer was right twice; where they diverge that is the point.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct Position {
    pub(crate) line: usize,
    pub(crate) column: usize,
}

/// A prepared index over one document. Building it is O(bytes). A lookup
/// is a binary search, then a column: arithmetic when the document is
/// ASCII, and a bounded scan from the nearest checkpoint when it is not.
pub(crate) struct PositionIndex<'a> {
    content: &'a str,
    /// Byte offset of the first character of each line.
    line_starts: Vec<usize>,
    /// `(byte offset, UTF-16 code units before it)`, every
    /// `CHECKPOINT_BYTES` or so.
    ///
    /// **Empty when the document is ASCII**, where a byte offset *is* a
    /// UTF-16 offset and a column is arithmetic. Without the
    /// checkpoints, a non-ASCII document re-counts code units from the
    /// line start on every lookup — invisible on a config file,
    /// quadratic on a minified bundle whose content sits on one very
    /// long line and carries a single accented character. Measured on
    /// the release binary before this index existed: 10,000 identifiers
    /// on one such line took 1.11 s and 20,000 took 4.03 s, which is the
    /// shape of a square.
    checkpoints: Vec<(usize, usize)>,
}

/// How far a lookup may have to scan. Small enough that the scan is
/// irrelevant, large enough that the index is a rounding error on a
/// document's size.
const CHECKPOINT_BYTES: usize = 1024;

impl<'a> PositionIndex<'a> {
    pub(crate) fn new(content: &'a str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            content
                .bytes()
                .enumerate()
                .filter(|&(_, byte)| byte == b'\n')
                .map(|(index, _)| index + 1),
        );
        Self {
            content,
            line_starts,
            checkpoints: checkpoints(content),
        }
    }

    /// The position of a byte offset. Offsets past the end clamp to the
    /// end, and an offset landing inside a multi-byte character floors
    /// to that character's start — neither can happen from a candidate
    /// span, but a silently wrong column would be worse than a
    /// defensive floor.
    pub(crate) fn at(&self, offset: usize) -> Position {
        let clamped = self.floor_to_boundary(offset.min(self.content.len()));
        let line_index = self.line_starts.partition_point(|&start| start <= clamped) - 1;
        let line_start = self.line_starts[line_index];
        Position {
            line: line_index + 1,
            column: self.units_before(clamped) - self.units_before(line_start) + 1,
        }
    }

    /// UTF-16 code units before a byte offset, counted from the nearest
    /// checkpoint at or below it.
    fn units_before(&self, offset: usize) -> usize {
        // ASCII: one byte, one code unit, and no index was built.
        let Some(&(byte, units)) = self.checkpoints.get(
            self.checkpoints
                .partition_point(|(at, _)| *at <= offset)
                .wrapping_sub(1),
        ) else {
            return offset;
        };
        units + self.content[byte..offset].encode_utf16().count()
    }

    fn floor_to_boundary(&self, mut offset: usize) -> usize {
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }
}

/// Running UTF-16 counts at character boundaries roughly
/// `CHECKPOINT_BYTES` apart. Empty for an ASCII document, which needs
/// none.
fn checkpoints(content: &str) -> Vec<(usize, usize)> {
    if content.is_ascii() {
        return Vec::new();
    }
    let mut out = vec![(0, 0)];
    let mut units = 0;
    let mut next = CHECKPOINT_BYTES;
    for (offset, character) in content.char_indices() {
        if offset >= next {
            out.push((offset, units));
            next = offset + CHECKPOINT_BYTES;
        }
        units += character.len_utf16();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_character_is_line_one_column_one() {
        assert_eq!(
            PositionIndex::new("abc").at(0),
            Position { line: 1, column: 1 }
        );
    }

    #[test]
    fn a_newline_starts_the_next_line() {
        let index = PositionIndex::new("ab\ncd");
        assert_eq!(index.at(3), Position { line: 2, column: 1 });
        assert_eq!(index.at(4), Position { line: 2, column: 2 });
    }

    #[test]
    fn an_empty_document_still_answers() {
        assert_eq!(
            PositionIndex::new("").at(0),
            Position { line: 1, column: 1 }
        );
    }

    #[test]
    fn an_offset_past_the_end_clamps() {
        assert_eq!(
            PositionIndex::new("ab").at(999),
            Position { line: 1, column: 3 }
        );
    }

    /// A two-byte character is one UTF-16 code unit, so the column after
    /// it advances by one, not two. Byte counting fails here.
    #[test]
    fn a_two_byte_character_counts_as_one_column() {
        assert_eq!(
            PositionIndex::new("é!").at(2),
            Position { line: 1, column: 2 }
        );
    }

    /// An astral character is a surrogate pair: two UTF-16 code units
    /// from four bytes. Counting Unicode scalars fails here, which is
    /// why the rule is UTF-16 and not "characters".
    #[test]
    fn an_astral_character_counts_as_two_columns() {
        assert_eq!(
            PositionIndex::new("🎯!").at(4),
            Position { line: 1, column: 3 }
        );
    }

    #[test]
    fn an_offset_inside_a_character_floors_to_its_start() {
        assert_eq!(
            PositionIndex::new("é!").at(1),
            Position { line: 1, column: 1 }
        );
    }

    /// The naive answer, written out once so the indexed one has
    /// something to be held to.
    fn counted(content: &str, offset: usize) -> Position {
        let line = content[..offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        let line_start = content[..offset].rfind('\n').map_or(0, |index| index + 1);
        Position {
            line: line + 1,
            column: content[line_start..offset].encode_utf16().count() + 1,
        }
    }

    /// The indexed path and the counted path must agree at **every**
    /// offset, or the index is a second implementation with its own
    /// answers. Run over an ASCII document, which builds no index at
    /// all, and a non-ASCII one long enough to cross several
    /// checkpoints.
    #[test]
    fn the_indexed_path_agrees_with_the_counted_path_everywhere() {
        let ascii = "abc\ndef\nghi";
        let mut long = String::new();
        for row in 0..400 {
            use std::fmt::Write as _;
            let _ = writeln!(long, "café {row} 🎯");
        }
        for content in [ascii, long.as_str()] {
            let index = PositionIndex::new(content);
            assert!(
                content.is_ascii() || index.checkpoints.len() > 3,
                "the long document must cross several checkpoints"
            );
            for offset in 0..=content.len() {
                if !content.is_char_boundary(offset) {
                    continue;
                }
                assert_eq!(index.at(offset), counted(content, offset), "at {offset}");
            }
        }
    }

    /// An ASCII document builds no index at all — the cheapest path is
    /// also the common one.
    #[test]
    fn an_ascii_document_needs_no_checkpoints() {
        assert!(PositionIndex::new("abc\ndef").checkpoints.is_empty());
    }

    /// One accented character in a megabyte of ASCII is the case that
    /// was quadratic: the whole document left the arithmetic path, and
    /// every lookup re-counted the line from its start. A checkpoint
    /// every kilobyte bounds the scan whatever the line length.
    #[test]
    fn a_long_line_carrying_one_non_ascii_character_is_still_bounded() {
        let content = format!("é{}", "a".repeat(100_000));
        let index = PositionIndex::new(&content);
        assert!(
            index.checkpoints.len() > 90,
            "a checkpoint roughly every {CHECKPOINT_BYTES} bytes"
        );
        assert_eq!(
            index.at(content.len()),
            Position {
                line: 1,
                column: 100_002,
            }
        );
    }

    /// A carriage return is an ordinary character, not a line break.
    #[test]
    fn a_carriage_return_does_not_start_a_line() {
        let index = PositionIndex::new("a\r\nb");
        assert_eq!(index.at(1), Position { line: 1, column: 2 });
        assert_eq!(index.at(3), Position { line: 2, column: 1 });
    }
}
