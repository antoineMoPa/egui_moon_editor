//! The minimap: the whole file drawn a few pixels tall down the editor's right edge, with the
//! stretch on screen marked, and a click or a drag on it scrolling the code there.
//!
//! It is fast because it never lays text out. What is kept per line is two numbers - how far
//! it is indented and where it ends - and what is kept per pixel row of the map is one span,
//! rebuilt only when the text has been changed or the map's height has. A frame that changes
//! neither paints one mesh of at most a screenful of rectangles, however long the file is.
//!
//! [`Minimap`] is the map itself, of anything laid out in rows: what it is of says where its
//! lines are and what ink each is in, and the map folds that to its own height, marks the
//! stretch on screen and turns a press into a scroll. The editor's is one of a text - see
//! [`TextMinimap`]; an application with rows of its own to map, a diff's say, draws through
//! the same one and so looks and answers the same.

use std::ops::Range;

use egui::{Color32, Id, Mesh, Rect, Sense, Ui, pos2, vec2};

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
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct MinimapSpan {
    start: u16,
    end: u16,
}

impl MinimapSpan {
    /// From one edge of the map to the other, which is what a band is.
    const ACROSS: Self = Self {
        start: 0,
        end: u16::MAX,
    };

    /// Where the ink of a line starts and ends, in characters, capped at `max_chars` - see
    /// [`Minimap::columns`].
    pub fn of(line: &str, max_chars: usize) -> Self {
        let trimmed = line.trim_end();
        let end = trimmed.chars().count();
        if end == 0 {
            return Self::default();
        }
        let start = trimmed.chars().take_while(|c| c.is_whitespace()).count();
        Self {
            start: start.min(max_chars) as u16,
            end: end.min(max_chars) as u16,
        }
    }

    fn is_empty(self) -> bool {
        self.end <= self.start
    }
}

/// A run of the map's pixel rows drawn the same: one rectangle of one ink.
struct Stroke {
    rows: Range<usize>,
    span: MinimapSpan,
    ink: Color32,
}

/// A map of something laid out in rows down a scroll area, and what it remembers between
/// frames. Each frame: [`scroll_asked`](Self::scroll_asked) before the scroll area runs,
/// [`fold`](Self::fold) if what is mapped has changed, and [`paint`](Self::paint) after.
#[derive(Default)]
pub struct Minimap {
    /// The picture: what [`fold`](Self::fold) last made.
    strokes: Vec<Stroke>,
    /// Pixels of map to a point of what it is of, as of the last fold.
    scale: f32,
    /// How tall what is mapped and the map itself were at the last fold.
    folded_for: (f32, f32),
    /// How tall the viewport was the last time the map was painted: what a click on the map
    /// is turned into a scroll offset with, before the scroll area has run this frame.
    viewport_height: f32,
    /// Whether a drag that began on the map is still going, so the pointer can leave the map's
    /// strip sideways without the code stopping following it.
    dragging: bool,
}

impl Minimap {
    /// How many characters of a line a map `map_width` wide has room for.
    pub fn columns(map_width: f32) -> usize {
        (map_width / CHAR_WIDTH) as usize
    }

    /// The scroll offset a press or drag on `map` asks for, so the scroll area can be given it
    /// before it runs. `None` while nothing on the map is being pressed. `id` is the map's
    /// own, the same one [`paint`](Self::paint) is given.
    pub fn scroll_asked(&mut self, ui: &Ui, id: Id, map: Rect) -> Option<f32> {
        let response = ui.interact(map, id, Sense::click_and_drag());
        let pressed = response.is_pointer_button_down_on() || response.dragged();
        self.dragging = pressed;
        if !pressed || self.scale == 0.0 {
            return None;
        }
        let y = response.interact_pointer_pos()?.y - map.min.y;
        // The pointed-at line lands in the middle of the screen, like the slider under it.
        Some((y / self.scale - self.viewport_height / 2.0).max(0.0))
    }

    /// Whether the picture is of something `content_height` tall on a map `map_height` tall.
    /// A caller folds again when this is not so, or when what it maps has itself changed.
    pub fn is_folded_for(&self, content_height: f32, map_height: f32) -> bool {
        self.folded_for == (content_height, map_height)
    }

    /// Start the picture over, of something `content_height` tall whose rows are `row_height`
    /// each, on a map `map_height` tall. What comes back is told the lines and the bands, a
    /// layer at a time, each drawn over the ones before it.
    pub fn fold(&mut self, content_height: f32, row_height: f32, map_height: f32) -> MinimapFold<'_> {
        self.strokes.clear();
        self.folded_for = (content_height, map_height);
        self.scale = (LINE_HEIGHT / row_height).min(map_height / content_height);
        let rows = ((content_height * self.scale).ceil() as usize).min(map_height.ceil() as usize);
        MinimapFold {
            strokes: &mut self.strokes,
            scale: self.scale,
            rows: vec![MinimapSpan::default(); rows],
        }
    }

    /// Paint the map into `map`, with the slider, in `slider_ink`, over the stretch from
    /// `offset` down.
    pub fn paint(
        &mut self,
        ui: &Ui,
        id: Id,
        map: Rect,
        slider_ink: Color32,
        offset: f32,
        viewport_height: f32,
    ) {
        self.viewport_height = viewport_height;

        // Dimmed while the pointer is elsewhere, so the map is a hint at the edge of the page
        // until it is wanted, and eased rather than snapped so it does not flicker as the
        // pointer crosses the code.
        let lit = self.dragging || ui.rect_contains_pointer(map);
        let brightness = ui.ctx().animate_value_with_time(
            id.with("brightness"),
            if lit { 1.0 } else { DIMMED_TO },
            FADE_SECONDS,
        );

        let mut mesh = Mesh::default();
        for stroke in &self.strokes {
            let left = f32::from(stroke.span.start) * CHAR_WIDTH;
            let right = (f32::from(stroke.span.end) * CHAR_WIDTH).min(map.width());
            mesh.add_colored_rect(
                Rect::from_min_max(
                    pos2(map.min.x + left, map.min.y + stroke.rows.start as f32),
                    pos2(map.min.x + right, map.min.y + stroke.rows.end as f32),
                ),
                stroke.ink.gamma_multiply(brightness),
            );
        }
        let painter = ui.painter_at(map);
        painter.add(mesh);

        let slider = Rect::from_min_size(
            pos2(map.min.x, map.min.y + offset * self.scale),
            vec2(map.width(), viewport_height * self.scale),
        )
        .intersect(map);
        painter.rect_filled(
            slider,
            0.0,
            slider_ink.gamma_multiply(brightness * if lit { 1.6 } else { 1.0 }),
        );
    }
}

/// A [`Minimap`]'s picture while it is being made - see [`Minimap::fold`].
pub struct MinimapFold<'a> {
    strokes: &'a mut Vec<Stroke>,
    scale: f32,
    /// One span per pixel row of the map, for the layer being folded.
    rows: Vec<MinimapSpan>,
}

impl MinimapFold<'_> {
    /// A layer of lines in one ink, each as how far down what is mapped its top is and the
    /// span of its text. A line is drawn on the one pixel row its top lands on, and a row
    /// several land on is the widest of them, from where the leftmost of them starts.
    pub fn lines(&mut self, ink: Color32, lines: impl IntoIterator<Item = (f32, MinimapSpan)>) {
        for (top, span) in lines {
            if span.is_empty() {
                continue;
            }
            let Some(row) = self.rows.get_mut((top * self.scale) as usize) else {
                continue;
            };
            *row = match row.is_empty() {
                true => span,
                false => MinimapSpan {
                    start: row.start.min(span.start),
                    end: row.end.max(span.end),
                },
            };
        }
        self.stroke_the_layer(ink);
    }

    /// A layer of bands in one ink, each from how far down what is mapped it starts to
    /// where it ends, and drawn across the whole map: the ground a stretch of lines sits on.
    /// A band is on every pixel row it covers, and on one at least.
    pub fn bands(&mut self, ink: Color32, bands: impl IntoIterator<Item = Range<f32>>) {
        for band in bands {
            let first = (band.start * self.scale) as usize;
            let end = ((band.end * self.scale) as usize)
                .max(first + 1)
                .min(self.rows.len());
            if let Some(rows) = self.rows.get_mut(first..end) {
                rows.fill(MinimapSpan::ACROSS);
            }
        }
        self.stroke_the_layer(ink);
    }

    /// Put the layer's rows into the picture, rows that are the same as one rectangle, and
    /// clear them for the next layer.
    fn stroke_the_layer(&mut self, ink: Color32) {
        let mut from = 0;
        while from < self.rows.len() {
            let span = self.rows[from];
            let same = self.rows[from..].iter().take_while(|row| **row == span).count();
            if !span.is_empty() {
                self.strokes.push(Stroke {
                    rows: from..from + same,
                    span,
                    ink,
                });
            }
            from += same;
        }
        self.rows.fill(MinimapSpan::default());
    }
}

/// The editor's own minimap: one of a text, a line of ink to a line of it.
#[derive(Default)]
pub(crate) struct TextMinimap {
    /// One span per line of the text, as of the last time the text was read.
    lines: Vec<MinimapSpan>,
    /// Whether the text has been changed since `lines` was read.
    stale: bool,
    /// The ink the map was last folded in, which a change of theme leaves it in until it
    /// is folded again.
    ink: Color32,
    map: Minimap,
}

impl TextMinimap {
    /// Read the text again before the next paint. Called wherever the buffer changes.
    pub(crate) fn text_changed(&mut self) {
        self.stale = true;
    }

    /// See [`Minimap::scroll_asked`].
    pub(crate) fn scroll_asked(&mut self, ui: &Ui, map: Rect) -> Option<f32> {
        self.map.scroll_asked(ui, Self::id(ui), map)
    }

    fn id(ui: &Ui) -> Id {
        ui.id().with("moon-editor-minimap")
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
        let read_again = self.stale || self.lines.is_empty();
        if read_again {
            let max_chars = Minimap::columns(map.width());
            self.lines = text.lines().map(|line| MinimapSpan::of(line, max_chars)).collect();
            self.stale = false;
        }
        let text_height = self.lines.len() as f32 * row_height;
        if read_again
            || self.ink != style.minimap_ink
            || !self.map.is_folded_for(text_height, map.height())
        {
            self.ink = style.minimap_ink;
            let lines = self.lines.iter().enumerate();
            self.map
                .fold(text_height, row_height, map.height())
                .lines(self.ink, lines.map(|(line, span)| (line as f32 * row_height, *span)));
        }
        self.map.paint(
            ui,
            Self::id(ui),
            map,
            style.minimap_slider_ink,
            offset,
            viewport_height,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A text of `lines`, each a row of `row_height`, folded onto a map `map_height` tall:
    /// the span on each pixel row of it.
    fn folded(lines: &[MinimapSpan], row_height: f32, map_height: f32) -> Vec<MinimapSpan> {
        let mut map = Minimap::default();
        let text_height = lines.len() as f32 * row_height;
        let lines = lines.iter().enumerate();
        map.fold(text_height, row_height, map_height)
            .lines(Color32::WHITE, lines.map(|(line, span)| (line as f32 * row_height, *span)));
        let rows = map.strokes.iter().map(|stroke| stroke.rows.end).max().unwrap_or(0);
        let mut spans = vec![MinimapSpan::default(); rows];
        for stroke in &map.strokes {
            spans[stroke.rows.clone()].fill(stroke.span);
        }
        spans
    }

    #[test]
    fn a_line_is_its_indent_to_its_last_character() {
        let span = MinimapSpan::of("    let x = 1;  ", 100);
        assert_eq!((span.start, span.end), (4, 14));
        assert_eq!(MinimapSpan::of("   ", 100).end, 0);
        assert_eq!(MinimapSpan::of(&"x".repeat(500), 100).end, 100);
    }

    #[test]
    fn a_long_file_is_squeezed_into_the_rows_it_has() {
        let lines = vec![MinimapSpan { start: 2, end: 10 }; 1000];
        let rows = folded(&lines, 16.0, 100.0);
        assert_eq!(rows.len(), 100);
        assert!(rows.iter().all(|row| (row.start, row.end) == (2, 10)));
    }

    #[test]
    fn a_short_file_is_two_rows_a_line() {
        let lines = [MinimapSpan { start: 0, end: 5 }, MinimapSpan::default(), MinimapSpan { start: 1, end: 3 }];
        let rows = folded(&lines, 16.0, 100.0);
        // The ink of a line on its first row, and the row under it left as the gap.
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].end, 5);
        assert_eq!(rows[1].end, 0);
        assert_eq!(rows[3].end, 0);
        assert_eq!((rows[4].start, rows[4].end), (1, 3));
    }

    #[test]
    fn a_band_is_on_every_row_it_covers_and_a_layer_is_drawn_over_the_one_before() {
        let mut map = Minimap::default();
        let mut fold = map.fold(160.0, 16.0, 100.0);
        // Two pixels of map to a row of sixteen: the band is rows 2 to 8, and a sliver of
        // one is still a row.
        fold.bands(Color32::RED, [16.0..64.0, 100.0..101.0]);
        fold.lines(Color32::GREEN, [(32.0, MinimapSpan { start: 0, end: 4 })]);

        let drawn: Vec<(Range<usize>, Color32)> = map
            .strokes
            .iter()
            .map(|stroke| (stroke.rows.clone(), stroke.ink))
            .collect();
        assert_eq!(
            drawn,
            vec![(2..8, Color32::RED), (12..13, Color32::RED), (4..5, Color32::GREEN)]
        );
    }
}
