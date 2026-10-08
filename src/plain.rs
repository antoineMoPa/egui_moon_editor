//! Plain marks - marks laid into the text of an [`egui::TextEdit`] that is not an
//! [`Editor`](crate::Editor).
//!
//! An application has boxes of prose beside its editors - a note, a description - and a search
//! over one of them marks its matches the way a search over code does. The box stays the
//! `TextEdit` it was: [`marked_plain_text`] is what its layouter lays the text out with, and
//! [`select_current_mark`](crate::select_current_mark) puts its caret on the mark stepped to.
//!
//! A selection made that way lasts only while the box has the keyboard: a `TextEdit` without
//! it folds its selection down to the caret each time it is drawn, which leaves the caret at
//! the end of the mark. A caller that hands the box the keyboard afterwards and wants the mark
//! selected selects it again on the frame it does.

use egui::{TextFormat, Ui, text::LayoutJob};

use crate::{
    editor::{Marks, marked_spans},
    style::EditorStyle,
    text::char_ranges_to_bytes,
};

/// The text of a multiline [`egui::TextEdit`] laid out the way that box lays it out when
/// nothing has been asked of its look - the body font, the ink of a widget at rest, wrapped at
/// the width it was given - with every mark tinted behind what it covers and the current one
/// underlined as well, in the inks `style` gives the marks of an [`Editor`](crate::Editor).
///
/// It is for the box's own layouter, which is where a `TextEdit` asks how its text looks:
///
/// ```no_run
/// # fn frame(ui: &mut egui::Ui, notes: &mut String, query: &str) {
/// use egui_moon_editor::{EditorStyle, Marks};
///
/// let found = egui_moon_editor::matches_in(notes, query);
/// let marks = Marks { ranges: &found, current: 0, select_current: true };
/// let style = EditorStyle::from_visuals(ui.visuals());
/// let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
///     let job =
///         egui_moon_editor::marked_plain_text(ui, text.as_str(), &marks, &style, wrap_width);
///     ui.fonts_mut(|fonts| fonts.layout_job(job))
/// };
/// let mut output = egui::TextEdit::multiline(notes)
///     .layouter(&mut layouter)
///     .show(ui);
/// egui_moon_editor::select_current_mark(ui, &marks, &mut output);
/// # }
/// ```
///
/// The marks are character ranges of the text they were found in, and the box lays its text
/// out again after taking what was typed into it this frame - so `text` can be shorter than
/// that one. The marks `text` still has the characters for are drawn, and the rest are left out
/// for the frame.
pub fn marked_plain_text(
    ui: &Ui,
    text: &str,
    marks: &Marks<'_>,
    style: &EditorStyle,
    wrap_width: f32,
) -> LayoutJob {
    // What `TextEdit`'s own layouter sets a multiline box's text in, read the way it reads
    // them, so a box drawn with marks is the box it was without them.
    let font = egui::FontSelection::default().resolve(ui.style());
    let ink = ui
        .visuals()
        .override_text_color
        .unwrap_or_else(|| ui.visuals().widgets.inactive.text_color());
    let line_height =
        ui.fonts_mut(|fonts| fonts.row_height(&font)) + ui.spacing().extra_text_line_spacing;
    let plain = TextFormat {
        line_height: Some(line_height),
        ..TextFormat::simple(font, ink)
    };

    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    job.keep_trailing_whitespace = true;

    let characters = text.chars().count();
    let held = marks
        .ranges
        .iter()
        .take_while(|range| range.end <= characters)
        .cloned();
    let byte_marks = char_ranges_to_bytes(text, held);
    for (span, look) in marked_spans(text, &byte_marks, marks.current, None, &[], style) {
        // An empty text keeps its one empty run: it is what gives an empty box a row to stand
        // the caret on.
        if span.is_empty() && !text.is_empty() {
            continue;
        }
        job.append(
            &text[span],
            0.0,
            TextFormat {
                background: look.background,
                underline: look.underline,
                ..plain.clone()
            },
        );
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matches_in;

    /// The runs of a job that a mark tinted, as the text each covers and whether it is
    /// underlined the way the current mark is.
    fn tinted(job: &LayoutJob) -> Vec<(&str, bool)> {
        job.sections
            .iter()
            .filter(|section| section.format.background != egui::Color32::TRANSPARENT)
            .map(|section| {
                (
                    &job.text[section.byte_range.start.0..section.byte_range.end.0],
                    section.format.underline != egui::Stroke::NONE,
                )
            })
            .collect()
    }

    /// Notes are prose, and prose has accents: a match is found by character and cut out of
    /// the text by byte, so a letter of two bytes ahead of a match must not slide the tint off
    /// the word it was found on.
    #[test]
    fn the_matches_are_tinted_where_they_are_and_the_current_one_underlined() {
        let text = "\u{c9}crire le parser cet \u{e9}t\u{e9}.\nLe Parser est lent.";
        let found = matches_in(text, "parser");
        let marks = Marks {
            ranges: &found,
            current: 1,
            select_current: false,
        };

        egui::__run_test_ui(|ui| {
            let job = marked_plain_text(ui, text, &marks, &EditorStyle::default(), 400.0);

            assert_eq!(job.text, text, "the box still says what was written in it");
            assert_eq!(tinted(&job), vec![("parser", false), ("Parser", true)]);
        });
    }

    /// The box lays its text out again after a selection is typed over, with the marks of the
    /// longer text it held a moment before: the one the text no longer reaches is left out
    /// rather than cut at characters that are not there.
    #[test]
    fn a_mark_past_the_end_of_a_text_typed_over_is_left_out() {
        let found = matches_in("parser and parser", "parser");
        let marks = Marks {
            ranges: &found,
            current: 0,
            select_current: false,
        };

        egui::__run_test_ui(|ui| {
            let typed_over = "parser \u{e9}";
            let job = marked_plain_text(ui, typed_over, &marks, &EditorStyle::default(), 400.0);

            assert_eq!(job.text, typed_over);
            assert_eq!(tinted(&job), vec![("parser", true)]);
        });
    }
}
