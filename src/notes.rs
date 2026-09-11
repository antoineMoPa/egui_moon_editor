//! Notes about stretches of the text, drawn in a column of their own left of the line
//! numbers: who last touched these lines, say, and when.
//!
//! A note covers a run of lines and is written once, at the top of the run - its title on the
//! run's first row and its detail on the second, when the run has one - with a rule across the
//! column where one run ends and the next begins. The caller works the notes out and says
//! what they read; the editor only lays them beside the lines they are about, so the same
//! column can carry a blame, a coverage report, or anything else that is a fact about a
//! stretch of lines.

use std::ops::Range;

use egui::Color32;

/// A note about one stretch of lines - see the [module](self) for how it is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct LineNote {
    /// The lines the note is about, as indexes from zero. The notes handed to the editor are
    /// in order and do not overlap.
    pub lines: Range<usize>,
    /// What the note reads at the top of its stretch, on the stretch's first row.
    pub title: String,
    /// The quieter second line of it, on the stretch's second row - which a stretch of one
    /// line has not got, so anything a reader has to see goes in the title.
    pub detail: String,
    /// The ink the title is written in, when it is not the style's own - a stretch that is
    /// not in any commit yet drawn in the colour the fringe marks new lines with, say.
    pub ink: Option<Color32>,
}

/// Between the edge of the column and the text in it, on either side, in points.
pub(crate) const NOTE_PADDING: f32 = 6.0;

/// How wide the column is, in characters: as wide as the widest note asks for, up to the
/// style's cap. Zero with no notes, which is a column that is not there.
pub(crate) fn column_chars(notes: &[LineNote], max_chars: usize) -> usize {
    notes
        .iter()
        .map(|note| note.title.chars().count().max(note.detail.chars().count()))
        .max()
        .unwrap_or(0)
        .min(max_chars)
}

/// A line of a note cut to the column, with an ellipsis where it ran on. `width` is in
/// characters, and a text within it is handed back as it is.
pub(crate) fn fitted(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let kept = width.saturating_sub(1);
    let mut cut: String = text.chars().take(kept).collect();
    cut.push('…');
    cut
}

/// Which note a line falls in, as an index into the notes, given that they are in order and
/// disjoint. Lines nobody wrote a note about have none.
pub(crate) fn note_at(notes: &[LineNote], line: usize) -> Option<usize> {
    let candidate = notes
        .partition_point(|note| note.lines.start <= line)
        .checked_sub(1)?;
    notes[candidate].lines.contains(&line).then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(lines: Range<usize>, title: &str, detail: &str) -> LineNote {
        LineNote {
            lines,
            title: title.to_string(),
            detail: detail.to_string(),
            ink: None,
        }
    }

    #[test]
    fn the_column_is_as_wide_as_the_widest_line_of_any_note_up_to_the_cap() {
        let notes = [note(0..2, "abc", "a longer detail"), note(2..3, "ab", "")];
        assert_eq!(column_chars(&notes, 40), 15);
        assert_eq!(column_chars(&notes, 8), 8);
        assert_eq!(column_chars(&[], 40), 0);
    }

    #[test]
    fn a_line_too_long_for_the_column_ends_in_an_ellipsis() {
        assert_eq!(fitted("short", 10), "short");
        assert_eq!(fitted("exactly ten", 11), "exactly ten");
        assert_eq!(fitted("far too long for it", 8), "far too…");
        // Multi-byte text is cut by character, never inside one.
        assert_eq!(fitted("héllo wörld", 6), "héllo…");
    }

    #[test]
    fn a_line_is_looked_up_in_the_note_that_covers_it() {
        let notes = [note(0..2, "a", ""), note(5..6, "b", "")];
        assert_eq!(note_at(&notes, 0), Some(0));
        assert_eq!(note_at(&notes, 1), Some(0));
        // A gap between notes is nobody's.
        assert_eq!(note_at(&notes, 2), None);
        assert_eq!(note_at(&notes, 4), None);
        assert_eq!(note_at(&notes, 5), Some(1));
        assert_eq!(note_at(&notes, 6), None);
        assert_eq!(note_at(&[], 0), None);
    }
}
