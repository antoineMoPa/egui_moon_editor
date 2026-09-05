//! The two answers a caller needs to send a place in the buffer somewhere else: the word under
//! the pointer, and where the caret sits.
//!
//! Everything here is asserted on what [`egui_moon_editor::EditorOutput`] said, rather than on
//! a picture of the frame: a widget crate that took a snapshot of itself would take a GPU into
//! the dev-dependencies of everyone who vendors it, for one image of an underline.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use egui_moon_editor::{Editor, EditorOutput, EditorRequest, EditorStyle, TextPoint, Word};

/// One line with a word in the middle of it, spaces either side, and a piece of punctuation
/// off on its own — the three things a pointer can land on.
const LINE: &str = "let value = other ;\n";

/// The modifier the tests hold, standing in for whatever the application picks: command on
/// macOS, ctrl elsewhere, which is exactly what `Modifiers::COMMAND` already means.
fn navigate_modifier() -> egui::Modifiers {
    egui::Modifiers::COMMAND
}

/// What a frame of the editor turned up, and the geometry the test needs to point at it: the
/// origin of the laid-out text and the width of one character of the monospace face it is set
/// in, so a test can say "over the sixth character" without knowing anything about the font.
#[derive(Default)]
struct Seen {
    navigable_word: Option<Word>,
    /// Kept once it has been seen, rather than replaced every frame: the click is reported on
    /// the one frame it happened, and the frames the harness runs after it would wipe it.
    navigated_to: Option<Word>,
    caret: Option<TextPoint>,
    origin: Option<egui::Pos2>,
    char_width: f32,
    row_height: f32,
}

impl Seen {
    /// Where on screen the `column`th character of the first line is, measured from the middle
    /// of the row so a rounding either way still lands on the text.
    fn over(&self, column: f32) -> egui::Pos2 {
        let origin = self.origin.expect("the text was never laid out");
        origin + egui::vec2(self.char_width * column, self.row_height * 0.5)
    }
}

/// An editor over `text`, drawn every frame with the navigate modifier the caller says, and
/// reporting into the [`Seen`] the harness holds as its state.
fn harness(text: &str, modifier: Option<egui::Modifiers>) -> Harness<'static, Seen> {
    let mut editor = Editor::new(text.to_string());
    Harness::builder()
        .with_size(egui::vec2(600.0, 300.0))
        .build_ui_state(
            move |ui, seen: &mut Seen| {
                let style = EditorStyle::from_visuals(ui.visuals());
                seen.char_width = ui.fonts_mut(|fonts| fonts.glyph_width(&style.font, 'm'));
                seen.row_height = ui.fonts_mut(|fonts| fonts.row_height(&style.font));
                let output: EditorOutput = editor.ui(
                    ui,
                    &style,
                    &EditorRequest {
                        // The first line asked about, which is how the test finds out where the
                        // text was laid out without measuring the fringe beside it.
                        line_of_interest: Some(1),
                        navigate_modifier: modifier,
                        ..Default::default()
                    },
                );
                seen.origin = output.line_at.map(|rect| rect.min);
                seen.navigable_word = output.navigable_word;
                seen.navigated_to = output.navigated_to.or(seen.navigated_to.take());
                seen.caret = output.caret;
            },
            Seen::default(),
        )
}

/// Hold the modifier from here on. It stays held until something says otherwise, the way a key
/// held down does.
fn hold(harness: &Harness<'_, Seen>, modifiers: egui::Modifiers) {
    harness.event(egui::Event::ModifiersChanged(modifiers));
}

/// Press and release the primary button where the pointer already is, a frame apart, so egui
/// sees a press and then a release rather than one indivisible blip.
fn click_at(harness: &mut Harness<'_, Seen>, pos: egui::Pos2, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        });
        harness.run_steps(2);
    }
}

#[test]
fn the_word_under_the_pointer_is_reported_only_while_the_modifier_is_held() {
    let mut harness = harness(LINE, Some(navigate_modifier()));
    harness.run_steps(4);
    let over_value = harness.state().over(5.5);

    // Pointing at it with nothing held is just pointing at text.
    harness.hover_at(over_value);
    harness.run_steps(3);
    assert_eq!(harness.state().navigable_word, None);

    hold(&harness, navigate_modifier());
    harness.hover_at(over_value);
    harness.run_steps(3);
    let word = harness
        .state()
        .navigable_word
        .clone()
        .expect("no word under the pointer with the modifier held");
    assert_eq!(word.text, "value");
    assert_eq!(
        word.at,
        TextPoint {
            offset: 4,
            line: 0,
            column: 4
        }
    );

    // And letting go of it puts the text back to being text.
    hold(&harness, egui::Modifiers::NONE);
    harness.run_steps(3);
    assert_eq!(harness.state().navigable_word, None);
}

#[test]
fn a_click_with_the_modifier_held_reports_the_word_and_a_plain_click_does_not() {
    let mut harness = harness(LINE, Some(navigate_modifier()));
    harness.run_steps(4);
    let over_value = harness.state().over(5.5);

    harness.hover_at(over_value);
    harness.run_steps(2);
    click_at(&mut harness, over_value, egui::Modifiers::NONE);
    assert_eq!(harness.state().navigated_to, None);

    hold(&harness, navigate_modifier());
    harness.hover_at(over_value);
    harness.run_steps(2);
    click_at(&mut harness, over_value, navigate_modifier());
    let clicked = harness
        .state()
        .navigated_to
        .clone()
        .expect("the modifier-click on the word was never reported");
    assert_eq!(clicked.text, "value");
    assert_eq!(clicked.at.offset, 4);
}

#[test]
fn a_pointer_on_whitespace_or_punctuation_is_over_no_word_at_all() {
    let mut harness = harness(LINE, Some(navigate_modifier()));
    harness.run_steps(4);
    hold(&harness, navigate_modifier());

    // The space between `let` and `value`, and the `;` sitting on its own at the end.
    for column in [3.5, 18.5] {
        let pos = harness.state().over(column);
        harness.hover_at(pos);
        harness.run_steps(3);
        assert_eq!(
            harness.state().navigable_word,
            None,
            "column {column} of {LINE:?} reported a word"
        );
    }

    // Past the end of the line, which the galley answers with the nearest place in the text -
    // the last word on it - unless where it was drawn is checked as well.
    let past_the_end = harness.state().over(40.0);
    harness.hover_at(past_the_end);
    harness.run_steps(3);
    assert_eq!(harness.state().navigable_word, None);
}

/// The units are spelled out on [`TextPoint`] and this is where that is worth anything: a line
/// with anything but ASCII on it counts more bytes than characters, and a caret reported in
/// characters would send a caller looking at the wrong place in its own copy of the text.
#[test]
fn the_caret_is_reported_in_bytes_on_a_line_that_is_not_ascii() {
    let mut harness = harness("", None);
    harness.run_steps(4);

    let text = harness.get_by_role(Role::MultilineTextInput);
    text.focus();
    text.type_text("h\u{e9}llo w\u{f6}rld");
    harness.run_steps(4);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("ab");
    harness.run_steps(4);

    // Eleven characters on the first line and thirteen bytes of them, so a caret counted in
    // characters would be two bytes short of where the text really goes on.
    let caret = harness.state().caret.clone().expect("no caret to report");
    assert_eq!(
        caret,
        TextPoint {
            offset: 16,
            line: 1,
            column: 2
        }
    );
}

/// The runs of the text as it was really laid out, taken from the shapes the frame painted:
/// what is under test is that the word the pointer is over reached the layout as an underline,
/// which is the only thing that makes it read as clickable before it is clicked.
fn runs(harness: &Harness<'_, Seen>) -> Vec<(String, egui::TextFormat)> {
    fn text_shapes(shape: &egui::Shape, found: &mut Vec<Vec<(String, egui::TextFormat)>>) {
        match shape {
            egui::Shape::Text(text) => {
                let job = &text.galley.job;
                if !job.text.starts_with(LINE) {
                    return;
                }
                found.push(
                    job.sections
                        .iter()
                        .map(|section| {
                            (
                                job.text[section.byte_range.start.0..section.byte_range.end.0]
                                    .to_string(),
                                section.format.clone(),
                            )
                        })
                        .collect(),
                );
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    text_shapes(shape, found);
                }
            }
            _ => {}
        }
    }

    let mut found = Vec::new();
    for clipped in &harness.output().shapes {
        text_shapes(&clipped.shape, &mut found);
    }
    found.pop().expect("the text was never painted")
}

/// The underline is a frame behind the pointer - the word is found by hit-testing text that has
/// already been laid out - so this is asserted after more than one frame of holding still.
#[test]
fn the_word_under_the_pointer_is_laid_out_underlined() {
    let mut harness = harness(LINE, Some(navigate_modifier()));
    harness.run_steps(4);
    let over_value = harness.state().over(5.5);

    hold(&harness, navigate_modifier());
    harness.hover_at(over_value);
    harness.run_steps(4);

    let underlined: Vec<String> = runs(&harness)
        .into_iter()
        .filter(|(_, format)| format.underline.width > 0.0)
        .map(|(text, _)| text)
        .collect();
    assert_eq!(underlined, vec!["value".to_string()]);
}
