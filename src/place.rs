//! Places in the buffer: where the caret is, and what word sits at a point in the text.
//!
//! Everything an editor is asked *about* rather than asked to draw ends up here. A caller with
//! somewhere to send a name — a definition, a symbol index, a list of things to finish a word
//! with — needs to be told where in the text it is and what the word there reads as, and that
//! is a question about a string and a cursor rather than about a widget. Keeping it beside the
//! drawing was making one long file of two unrelated jobs.

use std::ops::Range;

/// Where a place in the buffer sits: how far into the text it is, and the line it is on.
///
/// Bytes rather than characters, because the caller owns the text these offsets are into and
/// counts in whatever unit it answers to — a language server counts UTF-16 code units, a
/// `String` counts bytes — and it can only convert from a unit it is told. A second unit here
/// would not save that conversion, only add one more place to get it wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextPoint {
    /// How far into the buffer, in bytes.
    pub offset: usize,
    /// The line it is on, counting from zero.
    pub line: usize,
    /// How far into that line, in bytes.
    pub column: usize,
}

/// A word of the buffer, and where it starts.
///
/// Where it ends is `at.offset + text.len()` and is not kept: a second copy of the same fact
/// is a second thing to keep true as the buffer is typed into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    /// The text of it.
    pub text: String,
    /// Where its first character sits.
    pub at: TextPoint,
}

/// Where the caret sits in `text`, or nothing when the text area has never had one put in it.
///
/// The earlier end of a selection, not the caret proper: a selection about to be deleted is
/// edited from its start, wherever the caret sitting in it happens to be — and that is also
/// the end a caller asking about the place the caret is at wants to be told about.
///
/// The text area keeps its cursor while something else holds the keyboard, so this keeps
/// answering after the editor has been clicked out of: what it says is where typing would go
/// back to, which is what a line-and-column reading is of.
pub(crate) fn caret_at(ctx: &egui::Context, id: egui::Id, text: &str) -> Option<TextPoint> {
    let state = egui::text_edit::TextEditState::load(ctx, id)?;
    let range = state.cursor.char_range()?;
    let at = range.primary.index.0.min(range.secondary.index.0);
    Some(text_point(text, byte_of_char(text, at)))
}

/// A place in the text, worked out from how far into it in bytes it is.
///
/// The line breaks before the offset are what says which line it is on, and where the last of
/// them is says how far into that line it sits — in bytes again, so the two numbers are in the
/// same unit as the offset beside them.
pub(crate) fn text_point(text: &str, offset: usize) -> TextPoint {
    let before = &text[..offset];
    TextPoint {
        offset,
        line: before.matches('\n').count(),
        column: offset - before.rfind('\n').map_or(0, |at| at + 1),
    }
}

/// How far into `text` its `chars`th character is, in bytes; the end of the text for a count
/// past the end of it.
///
/// egui's text cursor counts characters and everything laid out is cut at bytes, so this is
/// the conversion between the two — the one place a caret read out of a text area crosses over
/// to the offsets the rest of this crate is written in.
pub(crate) fn byte_of_char(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(at, _)| at)
}

/// How many characters of `text` come before its `offset`th byte.
///
/// The way back from [`byte_of_char`], for the times the editor has worked something out in
/// bytes and has to say it to a text area that counts characters — putting the caret at the
/// end of what was just inserted, for one.
pub(crate) fn chars_before(text: &str, offset: usize) -> usize {
    text[..offset].chars().count()
}

/// What counts as one character of a word: what an identifier is made of in every language the
/// editor highlights, which is what makes a word here the thing worth navigating from, and the
/// thing worth finishing as it is typed.
pub(crate) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The word `offset` sits in, as a byte range of `text`, or nothing when it sits on a space or
/// a bracket — which is most of a page of code, and not something to offer a click on.
///
/// `offset` is a boundary between characters rather than a character, so the word is grown out
/// of it in both directions: a pointer over the middle of a word lands either side of the
/// letter it is on and has to find the same word from both.
pub(crate) fn word_around(text: &str, offset: usize) -> Option<Range<usize>> {
    let back: usize = text[..offset]
        .chars()
        .rev()
        .take_while(|c| is_word_char(*c))
        .map(char::len_utf8)
        .sum();
    let forward: usize = text[offset..]
        .chars()
        .take_while(|c| is_word_char(*c))
        .map(char::len_utf8)
        .sum();
    let range = offset - back..offset + forward;
    (!range.is_empty()).then_some(range)
}

/// The word being typed at `offset`: the run of word characters immediately before it, with
/// nothing of the word carrying on past it.
///
/// The middle of an identifier is not a word being typed — the caret sitting inside `value`
/// is somebody reading it, not somebody halfway through writing it — so a list of things to
/// finish it with would be offered over text that is already finished. Hence the end-of-word
/// condition, which is the whole difference between this and [`word_around`].
pub(crate) fn word_before(text: &str, offset: usize) -> Option<Word> {
    let carries_on = text[offset..].chars().next().is_some_and(is_word_char);
    if carries_on {
        return None;
    }
    let range = word_around(text, offset)?;
    Some(Word {
        text: text[range.clone()].to_string(),
        at: text_point(text, range.start),
    })
}

/// The word under `pointer`, given the text as it was laid out and where on screen that
/// laying-out was put.
///
/// The galley answers a position with the nearest place in the text, which is an answer even
/// for a pointer well off the end of a line — so the word it turns up is then checked against
/// where it was actually drawn. Without that, the space to the right of a short line reports
/// the last word on it, and every gap in a page of code looks clickable.
pub(crate) fn word_at(
    text: &str,
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    pointer: egui::Pos2,
) -> Option<Word> {
    let local = pointer - galley_pos.to_vec2();
    let at = byte_of_char(text, galley.cursor_from_pos(local.to_vec2()).index.0);
    let range = word_around(text, at)?;

    let first = chars_before(text, range.start);
    let last = first + text[range.clone()].chars().count();
    let drawn = galley
        .pos_from_cursor(egui::text::CCursor::new(first))
        .union(galley.pos_from_cursor(egui::text::CCursor::new(last)));
    if !drawn.contains(local) {
        return None;
    }

    Some(Word {
        text: text[range.clone()].to_string(),
        at: text_point(text, range.start),
    })
}

/// Where a word found under the pointer sits in the text now, or nothing when the text has
/// moved out from under it — which is what typing a line above it does.
pub(crate) fn word_still_at(text: &str, word: &Word) -> Option<Range<usize>> {
    let range = word.at.offset..word.at.offset + word.text.len();
    (text.get(range.clone()) == Some(word.text.as_str())).then_some(range)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_in_the_text_counts_the_line_breaks_before_it() {
        let text = "one\ntwo\nthree";
        assert_eq!(
            text_point(text, 9),
            TextPoint {
                offset: 9,
                line: 2,
                column: 1
            }
        );
    }

    #[test]
    fn the_word_being_typed_is_the_one_the_caret_sits_at_the_end_of() {
        let text = "let value = oth";
        assert_eq!(
            word_before(text, text.len()).map(|word| word.text),
            Some("oth".to_string())
        );
        // The middle of a finished word is somebody reading it, not writing it.
        assert_eq!(word_before(text, 6), None);
        // And a caret on a space is not typing a word at all.
        assert_eq!(word_before(text, 4), None);
    }
}
