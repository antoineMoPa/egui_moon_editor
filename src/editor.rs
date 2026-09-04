use std::ops::Range;

use egui::{Align, Rect, Response, Ui, pos2, vec2};

use crate::{style::EditorStyle, text::char_ranges_to_bytes};

/// A range of the text to tint, and whether it is the one the caller has stepped to.
///
/// Ranges are character ranges, the unit egui's text cursor counts in, so what a search hands
/// back from [`matches_in`](crate::matches_in) can be passed straight through.
#[derive(Clone, Debug, Default)]
pub struct Marks<'a> {
    /// The ranges to tint, in order and not overlapping.
    pub ranges: &'a [Range<usize>],
    /// Which of [`ranges`](Self::ranges) is the current one, counting from zero. Out of range
    /// means none of them is.
    pub current: usize,
    /// Whether to select the current range this frame and say where it landed.
    ///
    /// Only ask on the frame the caller has actually stepped: asking every frame drags the
    /// caret back to the mark, and the text cannot be typed into while marks are shown.
    pub select_current: bool,
}

/// What the caller wants of the editor this frame, beyond drawing the text.
#[derive(Clone, Debug, Default)]
pub struct EditorRequest<'a> {
    /// The ranges to tint into the text, and which of them is current.
    pub marks: Marks<'a>,
    /// A line to report the position of, one-based. The text has to be laid out before that
    /// can be measured, so the answer comes back in
    /// [`EditorOutput::line_at`](EditorOutput::line_at) rather than being asked for.
    ///
    /// The line is also what the editor scrolls to, ahead of the current mark.
    pub line_of_interest: Option<usize>,
    /// Whether the editor should take the keyboard this frame, so a tab brought forward can
    /// be typed into without clicking into the text first.
    pub focus: bool,
}

/// What drawing the editor turned up.
pub struct EditorOutput {
    /// The response of the text area, for a caller that wants to know whether the editor
    /// holds the keyboard or was clicked.
    pub response: Response,
    /// How many of the marks asked for were laid out into the text.
    pub marks_laid_out: usize,
    /// Where the current mark landed, when one was asked to be selected and there was one to
    /// select. Already scrolled to — reported so a caller can act on it further.
    pub current_mark_at: Option<Rect>,
    /// Where [`EditorRequest::line_of_interest`] was laid out, once there is text on screen
    /// to measure it in. Already scrolled to.
    pub line_at: Option<Rect>,
}

/// A text buffer and how it is drawn: a code editor, with a fringe of line numbers beside it.
///
/// The editor owns its text. State that belongs beside the text — the marks laid into it, and
/// in time everything a code editor keeps per line — lives here rather than in the caller, so
/// the caller is left with only where the text came from and where it goes.
///
/// ```no_run
/// # fn frame(ui: &mut egui::Ui, editor: &mut egui_moon_editor::Editor) {
/// let style = egui_moon_editor::EditorStyle::from_visuals(ui.visuals());
/// let output = editor.ui(ui, &style, &egui_moon_editor::EditorRequest::default());
/// # let _ = output;
/// # }
/// ```
pub struct Editor {
    text: String,
}

impl Editor {
    /// An editor over `text`.
    pub fn new(text: String) -> Self {
        Self { text }
    }

    /// The text as it stands, including whatever has been typed into it.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replace the text, the way loading a file does.
    pub fn set_text(&mut self, text: String) {
        self.text = text;
    }

    /// Draw the editor into `ui`, filling the space it was given.
    ///
    /// The fringe of line numbers is outside the sideways scroll, so the code slides under
    /// numbers that stay where they are, and inside the vertical one, so a number always sits
    /// beside its line.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        style: &EditorStyle,
        request: &EditorRequest<'_>,
    ) -> EditorOutput {
        let row_height = ui.fonts_mut(|fonts| fonts.row_height(&style.font));
        // Worked out before the text is lent to the `TextEdit`, and read from inside the
        // layouter, which is handed the same text back.
        let byte_marks = char_ranges_to_bytes(&self.text, request.marks.ranges.iter().cloned());
        let line_count = self.text.lines().count().max(1);

        let mut marks_laid_out = 0;
        let mut current_mark_at: Option<Rect> = None;
        let mut line_at: Option<Rect> = None;
        let mut response = None;

        egui::ScrollArea::vertical()
            .id_salt(ui.id().with("moon-editor"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    // A short file still gets an editor down to the bottom of the space, so
                    // the text sits on a page rather than in a box the size of what it holds.
                    let rows_on_screen = (ui.available_height() / row_height).floor() as usize;

                    // The fringe is outside the horizontal scroll area, so scrolling the code
                    // sideways slides it under numbers that stay where they are. Its height is
                    // only an estimate for layout - the numbers are painted where the laid-out
                    // text really put each line.
                    let fringe_height = row_height * line_count as f32;
                    let (fringe, _) = ui.allocate_exact_size(
                        vec2(style.fringe_width, fringe_height),
                        egui::Sense::hover(),
                    );
                    // The fringe's painter, kept from out here: the one inside the horizontal
                    // scroll area clips to the code, and the numbers sit left of it.
                    let painter = ui.painter().clone();

                    egui::ScrollArea::horizontal()
                        .id_salt(ui.id().with("moon-editor-code"))
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            // The marks are drawn into the text itself rather than left to the
                            // editor's selection: whatever is holding the keyboard while marks
                            // are shown, an unfocused editor paints no selection at all, so a
                            // search would otherwise turn up matches nobody can see.
                            let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                                let job = marked_text(
                                    text.as_str(),
                                    &byte_marks,
                                    request.marks.current,
                                    style,
                                    wrap,
                                );
                                ui.fonts_mut(|fonts| fonts.layout_job(job))
                            };
                            // A frame of its own, in place of the boxed-in one a `TextEdit`
                            // draws: no rounded corners, no border, and no accent-coloured ring
                            // when it holds the keyboard - the frame around the editor already
                            // shows that, and the text of a file should read as the page of an
                            // editor rather than as a form field on it.
                            // Nothing painted behind the text either: whatever background the
                            // editor was placed on carries through, so the code and the fringe
                            // of numbers beside it sit on one surface instead of the text being
                            // a panel on top.
                            let frame = egui::Frame::new().inner_margin(style.text_margin);
                            let output = egui::TextEdit::multiline(&mut self.text)
                                .font(style.font.clone())
                                .code_editor()
                                .frame(frame)
                                .margin(style.text_margin)
                                .desired_width(f32::INFINITY)
                                .desired_rows(line_count.max(rows_on_screen))
                                .layouter(&mut layouter)
                                .show(ui);
                            if request.focus {
                                output.response.request_focus();
                            }

                            // Each number at the height the galley actually gave its line -
                            // counting multiples of the font's row height drifts away from the
                            // text within a screen, because the editor lays its rows out with
                            // spacing of its own. A row only starts a line when the row before
                            // it ended in a newline, which is what keeps the numbers right if
                            // the text ever wraps.
                            let visible = painter.clip_rect().expand(row_height).y_range();
                            let mut line = 0;
                            let mut starts_line = true;
                            for placed in &output.galley.rows {
                                let starts = starts_line;
                                starts_line = placed.ends_with_newline;
                                if !starts {
                                    continue;
                                }
                                line += 1;
                                let y = output.galley_pos.y + placed.pos.y;
                                if Some(line) == request.line_of_interest {
                                    line_at = Some(Rect::from_min_size(
                                        pos2(output.galley_pos.x, y),
                                        vec2(1.0, row_height),
                                    ));
                                }
                                if !visible.contains(y) {
                                    continue;
                                }
                                painter.text(
                                    pos2(fringe.max.x - 6.0, y),
                                    egui::Align2::RIGHT_TOP,
                                    line.to_string(),
                                    style.line_number_font.clone(),
                                    style.fringe_ink,
                                );
                            }

                            marks_laid_out = request.marks.ranges.len();
                            response = Some(output.response.response.clone());
                            current_mark_at = select_current_mark(ui, &request.marks, output);
                        });

                    // Asked for out here, where the vertical scroll can hear it: the horizontal
                    // area around the code takes both axes' scroll targets so they cannot leak,
                    // and drops the one it has no bar for - so a mark below the fold, asked for
                    // from inside it, would never be scrolled to.
                    if let Some(rect) = line_at.or(current_mark_at) {
                        ui.scroll_to_rect(rect, Some(Align::Center));
                    }
                });
            });

        EditorOutput {
            // The text area is drawn on every path through the closure above, so there is
            // always one to report.
            response: response.expect("the text area is always drawn"),
            marks_laid_out,
            current_mark_at,
            line_at,
        }
    }
}

/// Select the current mark in the laid-out text and say where it landed, when the caller
/// asked for it this frame.
fn select_current_mark(
    ui: &mut Ui,
    marks: &Marks<'_>,
    mut output: egui::text_edit::TextEditOutput,
) -> Option<Rect> {
    if !marks.select_current {
        return None;
    }
    let range = marks.ranges.get(marks.current)?;

    let cursors = egui::text::CCursorRange::two(
        egui::text::CCursor::new(range.start),
        egui::text::CCursor::new(range.end),
    );
    let at = output
        .galley
        .pos_from_cursor(egui::text::CCursor::new(range.start))
        .translate(output.galley_pos.to_vec2());
    // Sideways from in here, where the code's own scroll can hear it.
    ui.scroll_to_rect(at, Some(Align::Center));

    output.state.cursor.set_char_range(Some(cursors));
    output.state.store(ui.ctx(), output.response.id);
    Some(at)
}

/// The text laid out with every mark tinted behind it, and the current one underlined as well,
/// so stepping between marks is visible without the others disappearing.
///
/// Marks are byte ranges of `text`. A mark reaching past the end of the text is where the
/// laying out stops: the text can have been edited since the marks were worked out, and the
/// rest of it is drawn plain rather than cut at an offset that is no longer there.
fn marked_text(
    text: &str,
    marks: &[Range<usize>],
    current: usize,
    style: &EditorStyle,
    wrap_width: f32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;

    let mut cut = 0;
    for (index, range) in marks.iter().enumerate() {
        if range.start < cut || range.end > text.len() || !text.is_char_boundary(range.start) {
            break;
        }
        job.append(&text[cut..range.start], 0.0, code_format(style, None));
        let mut format = code_format(style, Some(style.mark_ink));
        if index == current {
            format.underline = egui::Stroke::new(1.0, style.current_mark_ink);
        }
        job.append(&text[range.clone()], 0.0, format);
        cut = range.end;
    }
    job.append(&text[cut..], 0.0, code_format(style, None));
    job
}

/// One run of the editor's text: the font and ink the style asks for, over whatever the run is
/// marked with.
fn code_format(style: &EditorStyle, background: Option<egui::Color32>) -> egui::TextFormat {
    egui::TextFormat {
        font_id: style.font.clone(),
        color: style.ink,
        background: background.unwrap_or(egui::Color32::TRANSPARENT),
        ..Default::default()
    }
}
