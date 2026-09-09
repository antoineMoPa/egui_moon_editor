//! What the Tab key does: put one level of indentation in, rather than the tab character
//! egui's text area would type.
//!
//! Code is indented in whatever the repo it belongs to indents in — four spaces here, a tab
//! there — so what a press puts in is [`Indent`], said by the caller and not guessed at. The
//! same unit is what shift-tab takes back off.
//!
//! The presses are taken out of the frame's events before the text area runs, and the edit is
//! made here: the text and the caret in it are the editor's, and the text area is handed both
//! already changed. Which is also what keeps egui's own tab handling — a `\t` in, and a dedent
//! that only ever counts to four — from running at all.

use std::ops::Range;

use egui::{Key, Ui};

use crate::place::{byte_of_char, chars_before};

/// One level of indentation: what a Tab press puts in, and what shift-tab takes off.
///
/// A fact about the repo the file is in rather than about the person editing it, which is why
/// the editor is told rather than asked to have an opinion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Indent {
    /// One tab character per press.
    Tab,
    /// This many spaces per press. A width of zero indents by nothing, which is a repo that
    /// has configured itself out of indentation rather than anything this has to guard.
    Spaces(usize),
}

impl Default for Indent {
    /// Four spaces, which is what a file gets until a repo says otherwise.
    fn default() -> Self {
        Self::Spaces(4)
    }
}

impl Indent {
    /// The characters one press puts in.
    fn unit(self) -> String {
        match self {
            Self::Tab => "\t".to_string(),
            Self::Spaces(width) => " ".repeat(width),
        }
    }
}

/// Take this frame's Tab presses out of the events, apply them to `text`, and say which line
/// the earliest of them changed — the line the highlighter has to be read again from.
///
/// Nothing at all when no tab was pressed, and nothing when the presses changed no text:
/// shift-tab on a line with no indentation to take off is a press that did nothing.
///
/// The events are consumed whether or not they changed anything, so the tab never reaches the
/// text area behind this and never arrives as a `\t`.
pub(crate) fn take_tabs(ui: &Ui, id: egui::Id, text: &mut String, indent: Indent) -> Option<usize> {
    let mut presses = Vec::new();
    ui.input_mut(|input| {
        input.events.retain(|event| match event {
            egui::Event::Key {
                key: Key::Tab,
                pressed: true,
                modifiers,
                ..
            } => {
                presses.push(modifiers.shift);
                false
            }
            _ => true,
        });
    });
    if presses.is_empty() {
        return None;
    }

    // No state, or state with no cursor in it, is a text area that has never been typed in:
    // there is no place for the indentation to go, so the press does nothing.
    let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id)?;
    let cursor = state.cursor.char_range()?;
    let [min, max] = cursor.sorted_cursors();
    let mut selection = byte_of_char(text, min.index.0)..byte_of_char(text, max.index.0);

    let mut from_line: Option<usize> = None;
    for shift in presses {
        let change = apply(text, selection, indent, !shift);
        selection = change.selection;
        from_line = match (from_line, change.from_line) {
            (Some(earlier), Some(line)) => Some(earlier.min(line)),
            (earlier, line) => earlier.or(line),
        };
    }

    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(chars_before(text, selection.start)),
            egui::text::CCursor::new(chars_before(text, selection.end)),
        )));
    state.store(ui.ctx(), id);
    from_line
}

/// What one press did: where the selection ended up, and the first line it changed.
struct Change {
    /// The selection after the edit, in bytes, in the order the text has it.
    selection: Range<usize>,
    /// The first line the edit touched, counting from zero, or nothing when it touched none.
    from_line: Option<usize>,
}

/// Indent or dedent the text the selection covers, and say where the selection lands.
///
/// A caret with nothing selected indents at the caret, the way typing does. Anything selected
/// indents every line it touches, from its first character to its last — a selection ending
/// exactly at the start of a line does not reach into that line — because a block of code is
/// what a person with a selection is moving, not a range of characters to replace.
fn apply(text: &mut String, selection: Range<usize>, indent: Indent, deeper: bool) -> Change {
    let unit = indent.unit();
    if deeper && selection.is_empty() {
        text.insert_str(selection.start, &unit);
        let at = selection.start + unit.len();
        return Change {
            selection: at..at,
            from_line: (!unit.is_empty()).then(|| line_of(text, selection.start)),
        };
    }

    let mut starts = vec![line_start(text, selection.start)];
    while let Some(next) = text[*starts.last().expect("a line was just pushed")..]
        .find('\n')
        .map(|at| starts.last().expect("the same line") + at + 1)
        .filter(|next| *next < selection.end)
    {
        starts.push(next);
    }

    // Back to front: an edit at a line start moves everything after it, and the lines above
    // are where the offsets still hold.
    let mut deltas = vec![0isize; starts.len()];
    for (line, at) in starts.iter().enumerate().rev() {
        deltas[line] = match deeper {
            true => indent_line(text, *at, &unit),
            false => dedent_line(text, *at, unit.chars().count()),
        };
    }

    let first = starts[0];
    let last = starts.last().expect("a line was just pushed");
    let before_last: isize = deltas[..deltas.len() - 1].iter().sum();
    let total: isize = deltas.iter().sum();
    // Each end moves by what was put in or taken out ahead of it, and neither can be dragged
    // back past the start of the line it is on - a caret sitting in the spaces that were just
    // removed ends up where they were. A selection that began at the start of its line stays
    // there rather than sliding off the indentation it just put in: the block a person picked
    // out is still the block that is selected.
    let start = match selection.start == first {
        true => first,
        false => (selection.start as isize + deltas[0]).max(first as isize) as usize,
    };
    let end = (selection.end as isize + total).max(*last as isize + before_last) as usize;
    Change {
        selection: start..end.max(start),
        from_line: deltas
            .iter()
            .any(|delta| *delta != 0)
            .then(|| line_of(text, first)),
    }
}

/// Put one level in at the start of the line, and say how many bytes that added.
///
/// A blank line is left alone: indentation is what code sits after, and a line with no code
/// on it gets trailing spaces and nothing else.
fn indent_line(text: &mut String, at: usize, unit: &str) -> isize {
    if text[at..].starts_with('\n') || at == text.len() {
        return 0;
    }
    text.insert_str(at, unit);
    unit.len() as isize
}

/// Take one level off the start of the line, and say how many bytes that removed.
///
/// One tab, or up to `width` spaces — a line indented less than a full level loses what it
/// has, which is how a block whose lines are indented unevenly straightens out as it is
/// dedented rather than refusing to move.
fn dedent_line(text: &mut String, at: usize, width: usize) -> isize {
    let line = &text[at..];
    let taken = match line.starts_with('\t') {
        true => 1,
        false => line.chars().take(width).take_while(|c| *c == ' ').count(),
    };
    text.replace_range(at..at + taken, "");
    -(taken as isize)
}

/// Where the line `offset` is on starts.
fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |at| at + 1)
}

/// The line `offset` is on, counting from zero the way the highlighter counts them.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Indent or dedent text whose selection is written into it as `|` — one for a caret,
    /// two around a selection — and read the answer back the same way, so a test says what a
    /// person would see rather than a pair of offsets.
    fn press(marked: &str, indent: Indent, deeper: bool) -> String {
        let marks: Vec<usize> = marked.match_indices('|').map(|(at, _)| at).collect();
        let mut text = marked.replace('|', "");
        let selection = match marks[..] {
            [caret] => caret..caret,
            [start, end] => start..end - 1,
            _ => panic!("mark the selection with one `|` or two"),
        };

        let change = apply(&mut text, selection, indent, deeper);

        text.insert(change.selection.end, '|');
        if !change.selection.is_empty() {
            text.insert(change.selection.start, '|');
        }
        text
    }

    #[test]
    fn a_tab_at_the_caret_puts_four_spaces_in() {
        assert_eq!(
            press("fn one() {\n|}", Indent::default(), true),
            "fn one() {\n    |}"
        );
    }

    #[test]
    fn a_repo_that_indents_with_tabs_gets_a_tab() {
        assert_eq!(
            press("fn one() {\n|}", Indent::Tab, true),
            "fn one() {\n\t|}"
        );
    }

    #[test]
    fn a_selection_indents_every_line_it_touches() {
        assert_eq!(
            press("|one\ntwo\n|three", Indent::Spaces(2), true),
            "|  one\n  two\n|three"
        );
    }

    #[test]
    fn a_blank_line_in_the_block_is_left_alone() {
        assert_eq!(
            press("|one\n\ntwo|", Indent::Spaces(2), true),
            "|  one\n\n  two|"
        );
    }

    #[test]
    fn shift_tab_takes_one_level_off_the_line_the_caret_is_on() {
        assert_eq!(press("    one|", Indent::default(), false), "one|");
        assert_eq!(press("\tone|", Indent::Tab, false), "one|");
    }

    #[test]
    fn a_line_indented_less_than_a_level_loses_what_it_has() {
        assert_eq!(press("  one|", Indent::default(), false), "one|");
        assert_eq!(press("one|", Indent::default(), false), "one|");
    }

    #[test]
    fn a_dedent_leaves_the_caret_where_the_spaces_were() {
        assert_eq!(press("  |  one", Indent::default(), false), "|one");
    }
}
