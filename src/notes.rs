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

use egui::{Color32, Painter, Pos2, Rect, Response, vec2};

use crate::style::EditorStyle;

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
    /// A stretch of the title, in characters of it, that is a link - the hash of a commit,
    /// say: drawn in the style's link ink under a pointing hand while the pointer is on it,
    /// so it reads as the thing to click before it is clicked. The caller is told where a
    /// click landed either way - see [`NoteClick`].
    pub link: Option<Range<usize>>,
}

/// The note the pointer rests on, with the response of the stretch it was drawn over - what a
/// caller hangs a tooltip off, the way it would off any widget, so the whole of a note that
/// the column only had room for a line of comes up the way every other tooltip does: after
/// the pointer has rested, and not while it is moving.
#[derive(Clone, Debug)]
pub struct PointedNote {
    /// The note, as an index into the notes the editor was handed.
    pub note: usize,
    /// The response of the note's stretch on screen.
    pub response: Response,
}

/// Where in a note the pointer is, by row of the stretch and character column - what a click
/// or a hover is read as. See [`NoteClick`].
pub(crate) fn place_within(
    first_row: usize,
    rect: Rect,
    at: Pos2,
    row_height: f32,
    advance: f32,
) -> (usize, usize) {
    let rows_down = ((at.y - rect.min.y) / row_height).floor().max(0.0) as usize;
    let column = ((at.x - rect.min.x - NOTE_PADDING) / advance)
        .floor()
        .max(0.0) as usize;
    (first_row + rows_down, column)
}

/// Where a note's link sits on screen, given where its title starts, the font's advance and
/// the row's height.
pub(crate) fn link_rect(at: Pos2, link: &Range<usize>, advance: f32, height: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(at.x + link.start as f32 * advance, at.y),
        vec2((link.end - link.start) as f32 * advance, height),
    )
}

/// Paint a note's title at `at`, with the link lit in the style's link ink when `lit` names
/// one. The lit run is painted as a run of its own rather than over the title, so its edges
/// are its own colour rather than a blend of two.
pub(crate) fn paint_title(
    painter: &Painter,
    at: Pos2,
    title: &str,
    ink: Color32,
    lit: Option<Range<usize>>,
    style: &EditorStyle,
    advance: f32,
) {
    let Some(link) = lit else {
        painter.text(
            at,
            egui::Align2::LEFT_TOP,
            title,
            style.note_font.clone(),
            ink,
        );
        return;
    };
    let chars: Vec<char> = title.chars().collect();
    let end = link.end.min(chars.len());
    let start = link.start.min(end);
    let runs = [
        (0, &chars[..start], ink),
        (start, &chars[start..end], style.note_link_ink),
        (end, &chars[end..], ink),
    ];
    for (from, run, ink) in runs {
        painter.text(
            Pos2::new(at.x + from as f32 * advance, at.y),
            egui::Align2::LEFT_TOP,
            run.iter().collect::<String>(),
            style.note_font.clone(),
            ink,
        );
    }
}

/// Whether a place in a note - see [`place_within`] - is on its link.
pub(crate) fn on_the_link(note: &LineNote, row: usize, column: usize) -> bool {
    row == 0
        && note
            .link
            .as_ref()
            .is_some_and(|link| link.contains(&column))
}

/// Where in a note a click landed, so a caller can make different things of its parts - the
/// hash at the start of a title against the rest of it, say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteClick {
    /// The note, as an index into the notes the editor was handed.
    pub note: usize,
    /// The row of the note's stretch the click was on, from zero: the title's row is `0`, the
    /// detail's `1`.
    pub row: usize,
    /// The character column of the click within the note's text, from zero.
    pub column: usize,
}

/// Between the edge of the column and the text in it, on either side, in points.
pub(crate) const NOTE_PADDING: f32 = 6.0;

/// Where a click at `at` landed in a note drawn over `rect`, whose top row is row `first_row`
/// of the note's stretch - not always the first, since the top of a stretch can be scrolled
/// off. Rows are `row_height` tall and characters `advance` wide, the font being monospace.
pub(crate) fn click_within(
    note: usize,
    first_row: usize,
    rect: Rect,
    at: Pos2,
    row_height: f32,
    advance: f32,
) -> NoteClick {
    let (row, column) = place_within(first_row, rect, at, row_height, advance);
    NoteClick { note, row, column }
}

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
            link: None,
        }
    }

    #[test]
    fn the_link_is_a_stretch_of_the_title_row_only() {
        let mut linked = note(0..3, "abcdef1 someone", "");
        linked.link = Some(0..7);
        assert!(on_the_link(&linked, 0, 0));
        assert!(on_the_link(&linked, 0, 6));
        assert!(!on_the_link(&linked, 0, 7));
        assert!(
            !on_the_link(&linked, 1, 0),
            "the detail row is not the link"
        );
        assert!(
            !on_the_link(&note(0..3, "abcdef1", ""), 0, 0),
            "a note with no link"
        );
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
    fn a_click_is_placed_by_row_and_character_within_the_note() {
        let rect = Rect::from_min_max(egui::pos2(100.0, 200.0), egui::pos2(300.0, 260.0));
        // The first character of the title.
        assert_eq!(
            click_within(
                3,
                0,
                rect,
                egui::pos2(100.0 + NOTE_PADDING + 1.0, 201.0),
                20.0,
                8.0
            ),
            NoteClick {
                note: 3,
                row: 0,
                column: 0
            }
        );
        // Ten characters in, on the third row - which is row 7 of a stretch whose top is off
        // the screen and whose first row on it is row 5.
        assert_eq!(
            click_within(
                3,
                5,
                rect,
                egui::pos2(100.0 + NOTE_PADDING + 83.0, 250.0),
                20.0,
                8.0
            ),
            NoteClick {
                note: 3,
                row: 7,
                column: 10
            }
        );
        // In the padding left of the text still reads as the first character.
        assert_eq!(
            click_within(0, 0, rect, egui::pos2(101.0, 200.0), 20.0, 8.0).column,
            0
        );
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
