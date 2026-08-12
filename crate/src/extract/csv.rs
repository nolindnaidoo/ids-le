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

pub(crate) fn key_spans(text: &str) -> Vec<KeySpan> {
    let mut rows = lines(text).filter(|(_, line)| !line.trim().is_empty());
    let Some((_, header_line)) = rows.next() else {
        return Vec::new();
    };
    let headers: Vec<String> = fields(header_line)
        .into_iter()
        .map(|field| header_line[field].trim().trim_matches('"').to_string())
        .collect();

    let mut spans = Vec::new();
    for (offset, line) in rows {
        for (column, field) in fields(line).into_iter().enumerate() {
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
fn fields(line: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut quoted = false;

    for (at, byte) in bytes.iter().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b',' if !quoted => {
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
        let spans = key_spans(text);
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

    #[test]
    fn the_header_row_is_not_itself_data() {
        assert!(key_spans("name,channel_id\n").is_empty());
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
        assert!(key_spans("").is_empty());
        assert!(key_spans("\n\n").is_empty());
    }

    #[test]
    fn the_spans_are_ordered_and_do_not_overlap() {
        let spans = key_spans("a,b\n1,2\n3,4\n");
        assert_eq!(spans.len(), 4);
        for pair in spans.windows(2) {
            assert!(pair[0].end <= pair[1].start, "{pair:?}");
        }
    }
}
