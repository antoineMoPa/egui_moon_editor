//! The minimap scrolls the code: a press near the bottom of it brings the end of the file up.

use egui_kittest::Harness;
use egui_moon_editor::{Editor, EditorRequest, EditorStyle};

/// The line the top of the text area shows, read off where the pointer over it lands.
#[derive(Default)]
struct Seen {
    line_under_the_pointer: Option<usize>,
}

#[test]
fn pressing_the_bottom_of_the_map_scrolls_to_the_end_of_the_file() {
    let text: String = (1..=500).map(|n| format!("line {n}\n")).collect();
    let mut editor = Editor::new(text);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 300.0))
        .build_ui_state(
            move |ui, seen: &mut Seen| {
                let style = EditorStyle::from_visuals(ui.visuals());
                let output = editor.ui(ui, &style, &EditorRequest::default());
                seen.line_under_the_pointer = output.pointed_at.map(|point| point.line);
            },
            Seen::default(),
        );
    harness.run();

    // 500 lines at two pixels is 1000px, more than the 300 the map has, so it is squeezed to
    // fit and its bottom edge is the end of the file.
    let map_x = 600.0 - 20.0;
    harness.input_mut().events.push(egui::Event::PointerMoved(egui::pos2(map_x, 295.0)));
    harness.run();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: egui::pos2(map_x, 295.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();
    harness.run();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: egui::pos2(map_x, 295.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();

    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(200.0, 20.0)));
    harness.run();
    let line = harness.state().line_under_the_pointer.expect("the pointer is over the text");
    assert!(line > 400, "the text still shows line {line}");
}
