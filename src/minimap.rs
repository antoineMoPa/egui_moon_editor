//! The minimap: the whole file drawn a few pixels tall down the editor's right edge, with the
//! stretch on screen marked, and a click or a drag on it scrolling the code there.
//!
//! It is fast because it never lays text out. What is kept per line is two numbers - how far
//! it is indented and where it ends - and what is kept per pixel row of the map is one span,
//! rebuilt only when the text has been changed or the map's height has. A frame that changes
//! neither paints one mesh of at most a screenful of rectangles, however long the file is.

use egui::{Mesh, Rect, Sense, Ui, pos2, vec2};

use crate::style::EditorStyle;

/// The tallest a line is drawn. A short file is not stretched down the whole page; it is
/// drawn at this height from the top, and only a file too long for it is squeezed to fit.
const LINE_HEIGHT: f32 = 2.0;

/// How bright the map is while the pointer is away from it, against one for while it is on it.
const DIMMED_TO: f32 = 0.45;

/// How long the map takes to go from one to the other.
const FADE_SECONDS: f32 = 0.15;

/// How wide a character is drawn, in pixels.
const CHAR_WIDTH: f32 = 0.8;

/// Where a line's ink starts and ends, in characters. Both are capped at what the map has
/// room for, so a very long line is a full-width one rather than a number that overflows.
#[derive(Clone, Copy, Default)]
struct Span {
    start: u16,
    end: u16,
}

/// What the minimap remembers between frames.
#[derive(Default)]
pub(crate) struct Minimap {
    /// One span per line of the text, as of the last time the text was read.
    lines: Vec<Span>,
    /// One span per pixel row of the map, folded from `lines` for `rows_for`.
    rows: Vec<Span>,
    /// The map's height, in pixels, that `rows` was folded for.
    rows_for: f32,
    /// Whether the text has been changed since `lines` was read.
    stale: bool,
    /// How tall the code's viewport and how tall a row of it were the last time it was drawn:
    /// what a click on the map is turned into a scroll offset with, before the scroll area has
    /// run this frame.
    viewport_height: f32,
    row_height: f32,
    /// Whether a drag that began on the map is still going, so the pointer can leave the map's
    /// strip sideways without the code stopping following it.
    dragging: bool,
}

impl Minimap {
    /// Read the text again before the next paint. Called wherever the buffer changes.
    pub(crate) fn text_changed(&mut self) {
        self.stale = true;
    }

    /// The scroll offset a press or drag on `map` asks for, so the scroll area can be given it
    /// before it runs. `None` while nothing on the map is being pressed.
    pub(crate) fn scroll_asked(&mut self, ui: &Ui, map: Rect) -> Option<f32> {
        let response = ui.interact(map, ui.id().with("moon-editor-minimap"), Sense::click_and_drag());
        let pressed = response.is_pointer_button_down_on() || response.dragged();
        self.dragging = pressed;
        if !pressed || self.row_height == 0.0 {
            return None;
        }
        let y = response.interact_pointer_pos()?.y - map.min.y;
        let line = y / self.pixels_per_line(map.height());
        // The pointed-at line lands in the middle of the screen, like the slider under it.
        Some((line * self.row_height - self.viewport_height / 2.0).max(0.0))
    }

    fn pixels_per_line(&self, map_height: f32) -> f32 {
        LINE_HEIGHT.min(map_height / self.lines.len().max(1) as f32)
    }

    /// Paint the map into `map`, with the slider over the stretch from `offset` down.
    pub(crate) fn paint(
        &mut self,
        ui: &Ui,
        map: Rect,
        text: &str,
        style: &EditorStyle,
        offset: f32,
        viewport_height: f32,
        row_height: f32,
    ) {
        self.viewport_height = viewport_height;
        self.row_height = row_height;
        let max_chars = (map.width() / CHAR_WIDTH) as usize;
        if self.stale || self.lines.is_empty() {
            self.lines = text.lines().map(|line| span_of(line, max_chars)).collect();
            self.stale = false;
            self.rows_for = 0.0;
        }
        let per_line = self.pixels_per_line(map.height());
        if self.rows_for != map.height() || self.rows.is_empty() {
            self.rows = fold_rows(&self.lines, per_line);
            self.rows_for = map.height();
        }

        // Dimmed while the pointer is elsewhere, so the map is a hint at the edge of the page
        // until it is wanted, and eased rather than snapped so it does not flicker as the
        // pointer crosses the code.
        let lit = self.dragging || ui.rect_contains_pointer(map);
        let brightness = ui.ctx().animate_value_with_time(
            ui.id().with("moon-editor-minimap-brightness"),
            if lit { 1.0 } else { DIMMED_TO },
            FADE_SECONDS,
        );

        let mut mesh = Mesh::default();
        for (row, span) in self.rows.iter().enumerate() {
            if span.end <= span.start {
                continue;
            }
            let y = map.min.y + row as f32;
            mesh.add_colored_rect(
                Rect::from_min_size(
                    pos2(map.min.x + f32::from(span.start) * CHAR_WIDTH, y),
                    vec2(f32::from(span.end - span.start) * CHAR_WIDTH, 1.0),
                ),
                style.minimap_ink.gamma_multiply(brightness),
            );
        }
        let painter = ui.painter_at(map);
        painter.add(mesh);

        let top = map.min.y + offset / row_height * per_line;
        let slider = Rect::from_min_size(
            pos2(map.min.x, top),
            vec2(map.width(), viewport_height / row_height * per_line),
        )
        .intersect(map);
        painter.rect_filled(
            slider,
            0.0,
            style.minimap_slider_ink.gamma_multiply(brightness * if lit { 1.6 } else { 1.0 }),
        );
    }
}

/// Where the ink of a line starts and ends, in characters, capped at `max_chars`.
fn span_of(line: &str, max_chars: usize) -> Span {
    let trimmed = line.trim_end();
    let end = trimmed.chars().count();
    if end == 0 {
        return Span::default();
    }
    let start = trimmed.chars().take_while(|c| c.is_whitespace()).count();
    Span {
        start: start.min(max_chars) as u16,
        end: end.min(max_chars) as u16,
    }
}

/// One span per pixel row of the map: the widest of the lines that fall in it, from where the
/// leftmost of them starts. A row with no ink is an empty span.
fn fold_rows(lines: &[Span], per_line: f32) -> Vec<Span> {
    let rows = (lines.len() as f32 * per_line).ceil() as usize;
    let mut folded = Vec::with_capacity(rows);
    let mut line = 0;
    for row in 0..rows {
        let until = (((row + 1) as f32 / per_line).ceil() as usize).min(lines.len());
        let mut span: Option<Span> = None;
        while line < until {
            let this = lines[line];
            line += 1;
            if this.end <= this.start {
                continue;
            }
            span = Some(match span {
                Some(so_far) => Span {
                    start: so_far.start.min(this.start),
                    end: so_far.end.max(this.end),
                },
                None => this,
            });
        }
        folded.push(span.unwrap_or_default());
    }
    folded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_its_indent_to_its_last_character() {
        let span = span_of("    let x = 1;  ", 100);
        assert_eq!((span.start, span.end), (4, 14));
        assert_eq!(span_of("   ", 100).end, 0);
        assert_eq!(span_of(&"x".repeat(500), 100).end, 100);
    }

    #[test]
    fn a_long_file_is_squeezed_into_the_rows_it_has() {
        let lines = vec![Span { start: 2, end: 10 }; 1000];
        let per_line = LINE_HEIGHT.min(100.0 / lines.len() as f32);
        let rows = fold_rows(&lines, per_line);
        assert_eq!(rows.len(), 100);
        assert!(rows.iter().all(|row| (row.start, row.end) == (2, 10)));
    }

    #[test]
    fn a_short_file_is_two_rows_a_line() {
        let lines = [Span { start: 0, end: 5 }, Span::default()];
        let rows = fold_rows(&lines, LINE_HEIGHT);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].end, 5);
        assert_eq!(rows[3].end, 0);
    }
}
