//! Key paths in CSV: the header row names the columns.
//!
//! Unlike the sibling crate, the first row **is** treated as a header
//! here, and the reason is the difference between the two products.
//! `numbers-le` refuses to infer one because a header row of names
//! simply yields no numbers, so inferring one buys nothing and can lose
//! a row of data. Here the header is the only key material a CSV has,
//! and `channel_id` at the top of a column is exactly the evidence that
//! decides whether the integers under it are identifiers.
//!
//! A column with no name in the header is `[n]`, matching the array
//! convention everywhere else.

use super::locate::{KeySpan, lines};

/// The byte between fields. Tab-separated files are the same grammar
/// with a different one, and reading them with a comma made the whole
/// header row a single column name — which is then the evidence that
/// decides whether a run is an identifier, so a `checksum` column named
/// an ObjectId and a `user_id` column refused one.
pub(crate) const COMMA: u8 = b',';
pub(crate) const TAB: u8 = b'\t';

pub(crate) fn key_spans(text: &str, delimiter: u8) -> Vec<KeySpan> {
    let mut rows = lines(text).filter(|(_, line)| !line.trim().is_empty());
    let Some((_, header_line)) = rows.next() else {
        return Vec::new();
    };
    let headers: Vec<String> = fields(header_line, delimiter)
        .into_iter()
        .map(|field| header_line[field].trim().trim_matches('"').to_string())
        .collect();

    let mut spans = Vec::new();
    for (offset, line) in rows {
        for (column, field) in fields(line, delimiter).into_iter().enumerate() {
            spans.push(KeySpan {
                start: offset + field.start,
                end: offset + field.end,
                path: headers
                    .get(column)
                    .filter(|name| !name.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("[{column}]")),
            });
        }
    }
    spans
}

/// The byte range of each field in one row.
///
/// A quoted field keeps its quotes in the range. They are not identifier
/// characters, so they cannot join a run to its neighbour, and trimming
/// them would mean two sets of offsets to keep straight.
fn fields(line: &str, delimiter: u8) -> Vec<std::ops::Range<usize>> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut quoted = false;

    for (at, byte) in bytes.iter().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            _ if *byte == delimiter && !quoted => {
                out.push(start..at);
                start = at + 1;
            }
            _ => {}
        }
    }
    out.push(start..bytes.len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyed(text: &str, expected: &[(&str, &str)]) {
        let spans = key_spans(text, COMMA);
        let found: Vec<(&str, &str)> = spans
            .iter()
            .map(|span| (text[span.start..span.end].trim(), span.path.as_str()))
            .collect();
        assert_eq!(found, expected, "{text}");
    }

    #[test]
    fn the_header_row_names_the_columns() {
        keyed(
            "name,channel_id\nalpha,1\n",
            &[("alpha", "name"), ("1", "channel_id")],
        );
    }

    /// The bug this reader shipped: a tab row split on commas is one
    /// field, so the entire header became one column name — and that
    /// name is the evidence deciding whether a run is an identifier.
    #[test]
    fn a_tab_row_is_columns_under_tab_and_one_column_under_comma() {
        let text = "checksum\tuser_id\nabc\tdef\n";
        let tabbed: Vec<String> = key_spans(text, TAB)
            .iter()
            .map(|span| span.path.clone())
            .collect();
        assert_eq!(tabbed, ["checksum", "user_id"]);
        let comma: Vec<String> = key_spans(text, COMMA)
            .iter()
            .map(|span| span.path.clone())
            .collect();
        assert_eq!(comma, ["checksum\tuser_id"]);
    }

    #[test]
    fn the_header_row_is_not_itself_data() {
        assert!(key_spans("name,channel_id\n", COMMA).is_empty());
    }

    #[test]
    fn a_column_with_no_header_name_is_indexed() {
        keyed("name,\nalpha,1\n", &[("alpha", "name"), ("1", "[1]")]);
    }

    #[test]
    fn a_row_wider_than_the_header_is_indexed_past_it() {
        keyed(
            "name\nalpha,extra\n",
            &[("alpha", "name"), ("extra", "[1]")],
        );
    }

    /// A comma inside quotes is part of the field, which is the whole
    /// reason CSV quotes anything.
    #[test]
    fn a_quoted_comma_does_not_split_a_field() {
        keyed("a,b\n\"x,y\",z\n", &[("\"x,y\"", "a"), ("z", "b")]);
    }

    #[test]
    fn blank_lines_are_not_rows() {
        keyed("a\n\n1\n\n2\n", &[("1", "a"), ("2", "a")]);
    }

    #[test]
    fn an_empty_document_has_no_spans() {
        assert!(key_spans("", COMMA).is_empty());
        assert!(key_spans("\n\n", COMMA).is_empty());
    }

    #[test]
    fn the_spans_are_ordered_and_do_not_overlap() {
        let spans = key_spans("a,b\n1,2\n3,4\n", COMMA);
        assert_eq!(spans.len(), 4);
        for pair in spans.windows(2) {
            assert!(pair[0].end <= pair[1].start, "{pair:?}");
        }
    }
}
