//! Finishing a word as it is typed: the list the caller fills, the keyboard it borrows from the
//! text under it, and what ends up in the buffer.
//!
//! Asserted on [`egui_moon_editor::EditorOutput`], on the buffer, and on the text the frame
//! really painted — never on a picture of it, for the same reason the rest of this crate's
//! tests are not: a widget crate that took a snapshot of itself would take a GPU into the
//! dev-dependencies of everyone who vendors it.

use std::sync::{Arc, Mutex};

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use egui_moon_editor::{Completion, Editor, EditorRequest, EditorStyle, TextPoint, Word};

/// What the caller is offering, so a test can change its mind between frames the way an
/// application working out candidates as the text is typed does.
type Offer = Arc<Mutex<Vec<Completion>>>;

/// A candidate that reads as its own label and puts its own label in, leaving the caret at the
/// end of it — which is most of them, and lets a test say what it means in one word.
fn candidate(label: &str) -> Completion {
    Completion {
        label: label.to_string(),
        detail: None,
        insert: label.to_string(),
        caret_back: 0,
    }
}

/// A candidate that puts more in than the caret belongs at the end of: a call, whose caret is
/// asked for `caret_back` bytes short of the end of what went in.
fn candidate_leaving_the_caret_back(label: &str, insert: &str, caret_back: usize) -> Completion {
    Completion {
        label: label.to_string(),
        detail: None,
        insert: insert.to_string(),
        caret_back,
    }
}

/// What a frame of the editor turned up. The two reports that only happen on one frame are
/// kept once seen, since the harness runs several frames past the press that caused them.
#[derive(Default)]
struct Seen {
    text: String,
    caret: Option<TextPoint>,
    taken: Option<Completion>,
    dismissed: bool,
    word_being_typed: Option<Word>,
    focused: bool,
}

/// An editor over an empty buffer, offering whatever the test has put in `offer`.
fn harness(offer: &Offer) -> Harness<'static, Seen> {
    let offer = Arc::clone(offer);
    let mut editor = Editor::new(String::new());
    Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(
            move |ui, seen: &mut Seen| {
                let offering = offer.lock().unwrap().clone();
                let style = EditorStyle::from_visuals(ui.visuals());
                let output = editor.ui(
                    ui,
                    &style,
                    &EditorRequest {
                        completions: &offering,
                        ..Default::default()
                    },
                );
                seen.text = editor.text().to_string();
                seen.caret = output.caret;
                seen.taken = output.completion_taken.or(seen.taken.take());
                seen.dismissed |= output.completion_dismissed;
                seen.word_being_typed = output.word_being_typed;
                seen.focused = output.response.has_focus();
            },
            Seen::default(),
        )
}

/// Type `text` into the editor, having first given it the keyboard.
fn type_into(harness: &mut Harness<'_, Seen>, text: &str) {
    let area = harness.get_by_role(Role::MultilineTextInput);
    area.focus();
    area.type_text(text);
    harness.run_steps(4);
}

/// Every string the frame painted, the code itself included: the rows of the list are painted
/// rather than laid out as labels, so this is how a test asks whether the list is on screen.
fn painted(harness: &Harness<'_, Seen>) -> Vec<String> {
    fn walk(shape: &egui::Shape, found: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => found.push(text.galley.job.text.clone()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &harness.output().shapes {
        walk(&clipped.shape, &mut found);
    }
    found
}

/// Whether the row reading `label` is on screen.
fn showing(harness: &Harness<'_, Seen>, label: &str) -> bool {
    painted(harness).iter().any(|painted| painted == label)
}

#[test]
fn nothing_is_offered_under_the_caret_until_the_caller_offers_something() {
    let offer: Offer = Arc::default();
    let mut harness = harness(&offer);
    harness.run_steps(4);
    type_into(&mut harness, "let val");
    assert!(!showing(&harness, "value_one"));

    *offer.lock().unwrap() = vec![candidate("value_one"), candidate("value_two")];
    harness.run_steps(4);
    assert!(showing(&harness, "value_one"));
    assert!(showing(&harness, "value_two"));
}

#[test]
fn stepping_down_and_pressing_enter_puts_the_second_candidate_in_place_of_the_word_typed() {
    let offer: Offer = Arc::new(Mutex::new(Vec::new()));
    let mut harness = harness(&offer);
    harness.run_steps(4);
    type_into(&mut harness, "let val");
    *offer.lock().unwrap() = vec![candidate("value"), candidate("values")];
    harness.run_steps(4);

    harness.key_press(egui::Key::ArrowDown);
    harness.run_steps(2);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(4);

    assert_eq!(harness.state().text, "let values");
    assert_eq!(
        harness.state().taken.as_ref().map(|item| item.label.clone()),
        Some("values".to_string())
    );
    // The caret is left at the end of what was put in, ready to be typed on from.
    assert_eq!(
        harness.state().caret.as_ref().map(|point| point.offset),
        Some("let values".len())
    );
}

/// The bug the whole design is arranged to prevent: arrowing through the candidates walking the
/// caret through the file, and Enter breaking the line instead of taking a row.
#[test]
fn the_arrows_and_enter_belong_to_the_list_rather_than_to_the_text_under_it() {
    let offer: Offer = Arc::new(Mutex::new(Vec::new()));
    let mut harness = harness(&offer);
    harness.run_steps(4);
    type_into(&mut harness, "one\ntwo\nval");
    // A candidate that reads as the word already typed, so a press that reaches the text is
    // the only thing that could change the buffer or move the caret.
    *offer.lock().unwrap() = vec![candidate("val")];
    harness.run_steps(4);

    let before = harness.state().caret.clone().expect("no caret to move");
    for key in [egui::Key::ArrowUp, egui::Key::ArrowDown, egui::Key::Enter] {
        harness.key_press(key);
        harness.run_steps(3);
        assert_eq!(
            harness.state().text,
            "one\ntwo\nval",
            "{key:?} reached the text"
        );
        assert_eq!(
            harness.state().caret,
            Some(before.clone()),
            "{key:?} moved the caret"
        );
    }
    // And the keyboard is still in the text, rather than tabbed away somewhere else.
    assert!(harness.state().focused);
}

#[test]
fn escape_puts_the_list_away_and_the_next_escape_reaches_the_text() {
    let offer: Offer = Arc::new(Mutex::new(Vec::new()));
    let mut harness = harness(&offer);
    harness.run_steps(4);
    type_into(&mut harness, "let val");
    // Kept on offer after the escape, the way a caller that has not heard yet would: putting
    // the list away has to be the editor's own answer rather than the caller withdrawing it.
    *offer.lock().unwrap() = vec![candidate("value")];
    harness.run_steps(4);
    assert!(showing(&harness, "value"));

    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    assert!(harness.state().dismissed, "the escape was never reported");
    assert!(!showing(&harness, "value"), "the list is still on screen");
    // The escape was the list's, so the text still holds the keyboard.
    assert!(harness.state().focused);

    harness.state_mut().dismissed = false;
    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    assert!(!harness.state().dismissed, "the list took a second escape");
    assert!(
        !harness.state().focused,
        "the second escape never reached the text"
    );
}

#[test]
fn typing_while_the_list_is_showing_still_reaches_the_text() {
    let offer: Offer = Arc::new(Mutex::new(Vec::new()));
    let mut harness = harness(&offer);
    harness.run_steps(4);
    type_into(&mut harness, "let val");
    *offer.lock().unwrap() = vec![candidate("value")];
    harness.run_steps(4);

    harness.get_by_role(Role::MultilineTextInput).type_text("u");
    harness.run_steps(4);

    assert_eq!(harness.state().text, "let valu");
    assert_eq!(harness.state().taken, None);
}

#[test]
fn the_word_being_typed_is_the_one_before_the_caret_and_nothing_on_a_space() {
    let offer: Offer = Arc::new(Mutex::new(Vec::new()));
    let mut harness = harness(&offer);
    harness.run_steps(4);
    type_into(&mut harness, "let val");

    assert_eq!(
        harness.state().word_being_typed,
        Some(Word {
            text: "val".to_string(),
            at: TextPoint {
                offset: 4,
                line: 0,
                column: 4
            },
        })
    );

    harness.get_by_role(Role::MultilineTextInput).type_text(" ");
    harness.run_steps(4);
    assert_eq!(harness.state().word_being_typed, None);
}

#[test]
fn a_list_that_shortens_under_the_highlight_still_takes_a_row_that_is_in_it() {
    let offer: Offer = Arc::new(Mutex::new(Vec::new()));
    let mut harness = harness(&offer);
    harness.run_steps(4);
    type_into(&mut harness, "let val");
    *offer.lock().unwrap() = vec![
        candidate("value_one"),
        candidate("value_two"),
        candidate("value_three"),
    ];
    harness.run_steps(4);
    harness.key_press(egui::Key::ArrowDown);
    harness.key_press(egui::Key::ArrowDown);
    harness.run_steps(3);

    // What a caller narrowing its candidates as the word grows does, with the highlight sitting
    // past the end of what is left.
    *offer.lock().unwrap() = vec![candidate("value_one")];
    harness.run_steps(3);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(4);

    assert_eq!(
        harness.state().taken.as_ref().map(|item| item.label.clone()),
        Some("value_one".to_string())
    );
    assert_eq!(harness.state().text, "let value_one");
}

/// What a caller that writes the parentheses of a call needs of the editor: the text goes in
/// whole and the caret is left inside them, ready for an argument, rather than after the
/// closing one where nothing can be typed.
#[test]
fn a_candidate_that_asks_for_the_caret_back_leaves_it_that_far_short_of_the_end() {
    let offer: Offer = Arc::new(Mutex::new(Vec::new()));
    let mut harness = harness(&offer);
    harness.run_steps(4);
    type_into(&mut harness, "let x = gre");
    *offer.lock().unwrap() = vec![candidate_leaving_the_caret_back("greet", "greet()", 1)];
    harness.run_steps(4);

    harness.key_press(egui::Key::Enter);
    harness.run_steps(4);

    assert_eq!(harness.state().text, "let x = greet()");
    assert_eq!(
        harness.state().caret.as_ref().map(|point| point.offset),
        Some("let x = greet(".len()),
        "the caret is not between the parentheses"
    );
}

/// The same by Tab rather than Enter, since both take a row — and a caller asking for nothing
/// back still gets exactly what it got before there was anything to ask for.
#[test]
fn tab_takes_a_candidate_too_and_one_asking_for_nothing_back_ends_up_at_the_end_as_ever() {
    let offer: Offer = Arc::new(Mutex::new(Vec::new()));
    let mut harness = harness(&offer);
    harness.run_steps(4);
    type_into(&mut harness, "gre");
    *offer.lock().unwrap() = vec![candidate_leaving_the_caret_back("greet", "greet()", 1)];
    harness.run_steps(4);
    harness.key_press(egui::Key::Tab);
    harness.run_steps(4);
    assert_eq!(harness.state().text, "greet()");
    assert_eq!(
        harness.state().caret.as_ref().map(|point| point.offset),
        Some("greet(".len())
    );

    harness.get_by_role(Role::MultilineTextInput).type_text(" val");
    harness.run_steps(4);
    *offer.lock().unwrap() = vec![candidate("value")];
    harness.run_steps(4);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(4);
    assert_eq!(harness.state().text, "greet( value)");
    assert_eq!(
        harness.state().caret.as_ref().map(|point| point.offset),
        Some("greet( value".len()),
        "a candidate asking for nothing back left the caret somewhere other than the end"
    );
}
