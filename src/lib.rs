//! A code editor widget for [egui](https://github.com/emilk/egui).
//!
//! egui's [`TextEdit`](egui::TextEdit) is a text box. This is the rest of what makes an
//! editor: a fringe of line numbers that scrolls down the page with the code and stays put as
//! the code slides sideways under it, a page that fills the space it was given rather than a
//! box the size of the text in it, marks tinted into the text, and a way to select a range
//! and be told where on screen it landed.
//!
//! # The seam
//!
//! The widget owns a buffer and how it is drawn. Where the text came from and where it goes
//! is the caller's, and so is anything a search bar would hold — the query, the keyboard, the
//! tally. [`Editor`] takes the ranges to tint as per-frame input and hands back how many it
//! laid out and where the current one landed.
//!
//! ```no_run
//! # fn frame(ui: &mut egui::Ui, editor: &mut egui_moon_editor::Editor, query: &str) {
//! use egui_moon_editor::{EditorRequest, EditorStyle, Marks};
//!
//! let found = egui_moon_editor::matches_in(editor.text(), query);
//! let style = EditorStyle::from_visuals(ui.visuals());
//! let output = editor.ui(
//!     ui,
//!     &style,
//!     &EditorRequest {
//!         marks: Marks { ranges: &found, current: 0, select_current: true },
//!         ..Default::default()
//!     },
//! );
//! println!("{} matches", output.marks_laid_out);
//! # }
//! ```
//!
//! A whole file open in a window is about thirty lines:
//!
//! ```sh
//! cargo run --example edit -- src/lib.rs
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::doc_markdown)]

mod editor;
mod style;
mod syntax;
mod text;

pub use editor::{Editor, EditorOutput, EditorRequest, Marks};
pub use style::EditorStyle;
pub use syntax::{Highlighter, Language, Token, TokenStyle, highlight};
pub use text::{byte_matches_in, match_index_on_line, matches_in};
