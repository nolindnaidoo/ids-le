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
/// ASCII, and a UTF-16 count of the current line's prefix when it is not.
pub(crate) struct PositionIndex<'a> {
    content: &'a str,
    /// Byte offset of the first character of each line.
    line_starts: Vec<usize>,
    /// Whether the whole document is ASCII, in which case a byte offset
    /// **is** a UTF-16 offset and a column is arithmetic rather than a
    /// scan. Without it, `at()` re-counts code units from the line start
    /// on every call — invisible on ordinary source, quadratic on a
    /// minified file whose content sits on one very long line.
    all_ascii: bool,
}

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
            all_ascii: content.is_ascii(),
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
        let prefix = &self.content[line_start..clamped];
        let column = if self.all_ascii {
            prefix.len() + 1
        } else {
            prefix.encode_utf16().count() + 1
        };
        Position {
            line: line_index + 1,
            column,
        }
    }

    fn floor_to_boundary(&self, mut offset: usize) -> usize {
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }
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

    /// The fast path and the counted path must agree at every offset, or
    /// the optimisation is a second implementation with its own answers.
    #[test]
    fn the_ascii_fast_path_agrees_with_the_counted_path() {
        let ascii = "abc\ndef";
        let index = PositionIndex::new(ascii);
        assert!(index.all_ascii);
        for offset in 0..=ascii.len() {
            let line_index = index.line_starts.partition_point(|&start| start <= offset) - 1;
            let line_start = index.line_starts[line_index];
            assert_eq!(
                index.at(offset),
                Position {
                    line: line_index + 1,
                    column: ascii[line_start..offset].encode_utf16().count() + 1,
                },
                "at offset {offset}"
            );
        }
    }

    /// A carriage return is an ordinary character, not a line break.
    #[test]
    fn a_carriage_return_does_not_start_a_line() {
        let index = PositionIndex::new("a\r\nb");
        assert_eq!(index.at(1), Position { line: 1, column: 2 });
        assert_eq!(index.at(3), Position { line: 2, column: 1 });
    }
}
