//! Finding a query in the text, in the two units the editor needs it in.
//!
//! egui's text cursor counts characters, so a range handed to the editor as a selection is a
//! character range. A layout job's runs are cut at byte offsets instead. Both are here, and
//! [`byte_matches_in`] is the character ranges of [`matches_in`] converted across.

use std::ops::Range;

/// Every place `query` appears in `text`, as character ranges — which is what egui's text
/// cursor counts in, so a match can be handed straight to the editor as a selection.
///
/// Matched without regard for case. An empty query matches nothing.
///
/// Matches never overlap: the search steps past each one it finds rather than to the next
/// character, so a query that can overlap itself — `".."` over `"..."` — turns up one match
/// rather than two sharing a character.
///
/// ```
/// let found = egui_moon_editor::matches_in("Cargo.toml", "cargo");
/// assert_eq!(found, vec![0..5]);
/// ```
pub fn matches_in(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let haystack: Vec<char> = text.to_lowercase().chars().collect();
    let needle: Vec<char> = query.to_lowercase().chars().collect();
    // A character that changes length when lowercased would put the character indexes out of
    // step with the text the editor holds, so that text is matched exactly instead.
    let (haystack, needle) = if haystack.len() == text.chars().count() {
        (haystack, needle)
    } else {
        (text.chars().collect(), query.chars().collect())
    };
    if needle.is_empty() || haystack.len() < needle.len() {
        return Vec::new();
    }

    // Stepped past each match rather than to the next start, so a query that overlaps
    // itself (like ".." over "...") doesn't turn up two matches sharing a character - which
    // would hand the layout job in `marked_text` a pair of ranges out of order.
    let mut matches = Vec::new();
    let mut start = 0;
    while start + needle.len() <= haystack.len() {
        if haystack[start..start + needle.len()] == needle[..] {
            matches.push(start..start + needle.len());
            start += needle.len();
        } else {
            start += 1;
        }
    }
    matches
}

/// The same matches [`matches_in`] finds, as byte ranges of the text — which is what a layout
/// job's runs are cut at, where the editor's cursor counts characters.
///
/// ```
/// let text = "let caf\u{e9} = 1;";
/// let found = egui_moon_editor::byte_matches_in(text, "caf\u{e9}");
/// assert_eq!(&text[found[0].clone()], "caf\u{e9}");
/// ```
pub fn byte_matches_in(text: &str, query: &str) -> Vec<Range<usize>> {
    char_ranges_to_bytes(text, matches_in(text, query).into_iter())
}

/// Character ranges of `text` as byte ranges of it. The ranges have to be inside the text and
/// in order, which is what [`matches_in`] hands back.
pub(crate) fn char_ranges_to_bytes(
    text: &str,
    ranges: impl Iterator<Item = Range<usize>>,
) -> Vec<Range<usize>> {
    let starts: Vec<usize> = text
        .char_indices()
        .map(|(at, _)| at)
        .chain(std::iter::once(text.len()))
        .collect();
    ranges
        .map(|range| starts[range.start]..starts[range.end])
        .collect()
}

/// Which match of the whole text the first one on `line` is, counting both from zero — which
/// is the index a caller stepping through matches addresses them by.
///
/// `line` is one-based, the way an editor numbers its lines. A line holding more than one
/// match answers with its first; a line holding none, or a line past the end, answers
/// `None`.
///
/// ```
/// let text = "one needle\nnothing\nneedle needle\nneedle\n";
/// assert_eq!(egui_moon_editor::match_index_on_line(text, "needle", 3), Some(1));
/// ```
pub fn match_index_on_line(text: &str, query: &str, line: usize) -> Option<usize> {
    let mut before = 0;
    for (index, text_of_line) in text.split_inclusive('\n').enumerate() {
        let on_this_line = matches_in(text_of_line, query).len();
        if index + 1 == line {
            return (on_this_line > 0).then_some(before);
        }
        before += on_this_line;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_match_on_a_line_is_counted_from_the_ones_above_it() {
        let text = "one needle\nnothing\nneedle needle\nneedle\n";

        assert_eq!(match_index_on_line(text, "needle", 1), Some(0));
        // The line above holds two of them, so the one below is the fourth.
        assert_eq!(match_index_on_line(text, "needle", 3), Some(1));
        assert_eq!(match_index_on_line(text, "needle", 4), Some(3));
    }

    #[test]
    fn a_line_without_the_query_on_it_has_no_match_to_step_to() {
        let text = "one needle\nnothing\n";

        assert_eq!(match_index_on_line(text, "needle", 2), None);
        assert_eq!(match_index_on_line(text, "needle", 9), None);
    }

    #[test]
    fn a_match_is_a_character_range_of_the_text() {
        let text = "fn greet() {}\nfn greet_again() {}\n";
        let found = matches_in(text, "greet");

        assert_eq!(found.len(), 2);
        let first = found[0].clone();
        assert_eq!(
            text.chars()
                .skip(first.start)
                .take(first.len())
                .collect::<String>(),
            "greet"
        );
    }

    #[test]
    fn case_is_not_what_a_search_is_about() {
        assert_eq!(matches_in("Cargo.toml", "cargo").len(), 1);
        assert_eq!(matches_in("Cargo.toml", "CARGO").len(), 1);
    }

    /// A query that overlaps itself, over a run of characters it can overlap with, used to
    /// turn up matches that shared a character - `target_env'...` searched for `..` found the
    /// first two dots and then the last two, one byte apart. `marked_text` cuts the text at
    /// each match in turn, so a pair like that put its second cut behind the first and
    /// panicked slicing the text - which is what closed the window the bar was open over.
    #[test]
    fn a_self_overlapping_query_does_not_turn_up_overlapping_matches() {
        let found = matches_in("target_env'...\" >&2", "..");

        assert_eq!(found, vec![11..13]);
    }

    /// A match past the first line has to count the newline, or the editor would put the
    /// caret somewhere else entirely.
    #[test]
    fn a_match_on_a_later_line_counts_the_line_breaks_before_it() {
        let text = "one\ntwo\nthree";
        let found = matches_in(text, "three");

        assert_eq!(found, vec![8..13]);
    }

    #[test]
    fn nothing_matches_an_empty_query_or_a_query_that_is_not_there() {
        assert!(matches_in("hello", "").is_empty());
        assert!(matches_in("hello", "absent").is_empty());
        assert!(matches_in("hi", "far too long").is_empty());
    }

    /// The marks are cut into the text by byte, while the caret counts characters. A line
    /// with anything but ASCII on it would land the marks somewhere else entirely if the two
    /// were mixed up.
    #[test]
    fn a_mark_is_the_bytes_of_the_text_the_match_covers() {
        let text = "let caf\u{e9} = \"caf\u{e9}\";\n";
        let found = byte_matches_in(text, "caf\u{e9}");

        assert_eq!(found.len(), 2);
        for range in found {
            assert_eq!(&text[range], "caf\u{e9}");
        }
    }
}
