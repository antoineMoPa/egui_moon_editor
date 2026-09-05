//! The widget driven through a real egui context: the fringe, typing, and marks.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use egui_moon_editor::{Editor, EditorRequest, EditorStyle, Marks};

/// Ten lines, so there is a fringe with more than one digit in it.
fn ten_lines() -> String {
    (1..=10)
        .map(|line| format!("pub const LINE_{line}: u32 = {line};\n"))
        .collect()
}

/// The line numbers are painted rather than laid out as widgets, so what is checked here is
/// that the text area is set in from the left edge by the width of the fringe beside it.
#[test]
fn the_text_is_drawn_beside_a_fringe_of_line_numbers() {
    let inset = std::sync::Arc::new(std::sync::Mutex::new(0.0f32));
    let out = std::sync::Arc::clone(&inset);

    let mut editor = Editor::new(ten_lines());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui(move |ui| {
            let left = ui.max_rect().left();
            let style = EditorStyle::from_visuals(ui.visuals());
            let output = editor.ui(ui, &style, &EditorRequest::default());
            *out.lock().unwrap() = output.response.rect.left() - left;
        });

    harness.run_steps(4);

    let style = EditorStyle::default();
    assert!(
        *inset.lock().unwrap() >= style.fringe_width,
        "the text runs over the fringe: inset {}, fringe {}",
        inset.lock().unwrap(),
        style.fringe_width
    );
    // And the whole of the text is there to be read.
    let text = harness.get_by_role(Role::MultilineTextInput);
    assert!(
        text.value()
            .unwrap()
            .contains("pub const LINE_10: u32 = 10;")
    );
}

/// The editor owns its text, so typing into it is what changes what `text()` says.
#[test]
fn typing_into_the_editor_changes_the_text_it_holds() {
    let text = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let out = std::sync::Arc::clone(&text);

    let mut editor = Editor::new("fn one() {}".to_string());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 200.0))
        .build_ui(move |ui| {
            let style = EditorStyle::from_visuals(ui.visuals());
            let output = editor.ui(ui, &style, &EditorRequest::default());
            output.response.request_focus();
            *out.lock().unwrap() = editor.text().to_string();
        });

    harness.run_steps(4);
    harness.get_by_role(Role::MultilineTextInput).type_text("!");
    harness.run_steps(4);

    assert!(
        text.lock().unwrap().contains('!'),
        "what was typed never reached the buffer: {:?}",
        text.lock().unwrap()
    );
}

/// The marks are input and the tally is output: the widget says how many it laid out, and
/// where the current one landed once it has been asked to select it.
#[test]
fn the_current_mark_is_selected_and_its_place_reported() {
    let laid_out = std::sync::Arc::new(std::sync::Mutex::new((0usize, false)));
    let out = std::sync::Arc::clone(&laid_out);

    let mut editor = Editor::new(ten_lines());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui(move |ui| {
            let found = egui_moon_editor::matches_in(editor.text(), "LINE_");
            let style = EditorStyle::from_visuals(ui.visuals());
            let output = editor.ui(
                ui,
                &style,
                &EditorRequest {
                    marks: Marks {
                        ranges: &found,
                        current: 3,
                        select_current: true,
                    },
                    ..Default::default()
                },
            );
            *out.lock().unwrap() = (output.marks_laid_out, output.current_mark_at.is_some());
        });

    harness.run_steps(4);

    let (total, placed) = *laid_out.lock().unwrap();
    assert_eq!(total, 10, "one mark per line");
    assert!(placed, "the current mark was never placed on screen");
}

/// A line asked about is reported where the laid-out text put it, which is what a caller
/// opening a file at a search hit scrolls to.
#[test]
fn the_line_of_interest_is_reported_where_it_was_laid_out() {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let out = std::sync::Arc::clone(&seen);

    let mut editor = Editor::new(ten_lines());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui(move |ui| {
            let style = EditorStyle::from_visuals(ui.visuals());
            let output = editor.ui(
                ui,
                &style,
                &EditorRequest {
                    line_of_interest: Some(7),
                    ..Default::default()
                },
            );
            *out.lock().unwrap() = output.line_at.map(|rect| rect.min.y);
        });

    harness.run_steps(4);

    let seventh = seen.lock().unwrap().expect("line 7 was never placed");
    assert!(
        seventh.is_finite(),
        "line 7 landed nowhere measurable: {seventh}"
    );
}

/// The runs of the code as it was really laid out on screen, taken from the shapes the frame
/// painted rather than from anything the widget was asked directly: what is under test is that
/// the highlighting reached the galley, which only happens if the editor worked out the window
/// on screen and read the grammar over it.
fn code_runs(harness: &egui_kittest::Harness<'_>) -> Vec<(String, egui::TextFormat)> {
    fn text_shapes(shape: &egui::Shape, found: &mut Vec<Vec<(String, egui::TextFormat)>>) {
        match shape {
            egui::Shape::Text(text) => {
                let job = &text.galley.job;
                if !job.text.contains("pub const LINE_1") {
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
    found.pop().expect("the code was never painted")
}

/// The whole point of the feature, seen from outside: a Rust file is not one colour.
#[cfg(feature = "syntax")]
#[test]
fn a_rust_buffer_is_laid_out_in_runs_of_more_than_one_colour() {
    let mut editor = Editor::new(ten_lines());
    editor.set_language(egui_moon_editor::Language::of_path("src/lib.rs"));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui(move |ui| {
            let style = EditorStyle::from_visuals(ui.visuals());
            editor.ui(ui, &style, &EditorRequest::default());
        });

    harness.run_steps(4);

    let runs = code_runs(&harness);
    let inks: std::collections::BTreeSet<[u8; 4]> = runs
        .iter()
        .map(|(_, format)| format.color.to_array())
        .collect();
    assert!(
        inks.len() > 1,
        "the code was laid out in one colour: {inks:?}"
    );
    let keyword = runs
        .iter()
        .find(|(text, _)| text.starts_with("pub"))
        .expect("no run began at the `pub` that starts every line");
    let name = runs
        .iter()
        .find(|(text, _)| text.contains("LINE_1"))
        .expect("no run held a constant's name");
    assert_ne!(keyword.1.color, name.1.color);
}

/// The composition of the two things drawn into the text: a match found by a search is still
/// tinted once the code under it has a colour of its own. Runs true with the feature off as
/// well, where the code has one colour and the marks are all there is.
#[test]
fn a_search_over_highlighted_code_still_marks_every_match() {
    let mut editor = Editor::new(ten_lines());
    editor.set_language(egui_moon_editor::Language::of_path("src/lib.rs"));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui(move |ui| {
            let found = egui_moon_editor::matches_in(editor.text(), "LINE_");
            let style = EditorStyle::from_visuals(ui.visuals());
            editor.ui(
                ui,
                &style,
                &EditorRequest {
                    marks: Marks {
                        ranges: &found,
                        current: 0,
                        select_current: false,
                    },
                    ..Default::default()
                },
            );
        });

    harness.run_steps(4);

    let style = EditorStyle::default();
    let runs = code_runs(&harness);
    let marked: Vec<&(String, egui::TextFormat)> = runs
        .iter()
        .filter(|(_, format)| format.background == style.mark_ink)
        .collect();
    // Ten lines, one `LINE_` on each, and every one of them tinted end to end - a match cut
    // in two by a change of colour under it would show up as more runs than matches.
    assert_eq!(marked.len(), 10, "{marked:?}");
    for (text, _) in &marked {
        assert_eq!(text, "LINE_");
    }
    assert!(
        marked[0].1.underline.width > 0.0,
        "the current match was not underlined"
    );
    // And nothing else was tinted: the code around the matches is drawn on the page it sits on.
    assert!(
        runs.iter()
            .filter(|(_, format)| format.background != egui::Color32::TRANSPARENT)
            .count()
            == 10
    );
}
