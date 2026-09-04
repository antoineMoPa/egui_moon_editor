//! A file open in a window: `cargo run --example edit -- src/lib.rs`.
//!
//! Type into it and press `⌘S` (`ctrl+S` off macOS) to write it back.

use egui_moon_editor::{Editor, EditorRequest, EditorStyle};

struct EditWindow {
    path: std::path::PathBuf,
    editor: Editor,
    status: String,
}

impl eframe::App for EditWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            let save =
                ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
            if save {
                self.status = match std::fs::write(&self.path, self.editor.text()) {
                    Ok(()) => format!("wrote {}", self.path.display()),
                    Err(error) => format!("{error}"),
                };
            }
            ui.horizontal(|ui| {
                ui.label(self.path.display().to_string());
                ui.label(&self.status);
            });
            let style = EditorStyle::from_visuals(ui.visuals());
            self.editor.ui(ui, &style, &EditorRequest::default());
        });
    }
}

fn main() -> eframe::Result<()> {
    let path = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "Cargo.toml".to_string()),
    );
    let text = std::fs::read_to_string(&path).unwrap_or_default();

    eframe::run_native(
        "egui_moon_editor",
        eframe::NativeOptions::default(),
        Box::new(move |_cc| {
            Ok(Box::new(EditWindow {
                path,
                editor: Editor::new(text),
                status: String::new(),
            }))
        }),
    )
}
