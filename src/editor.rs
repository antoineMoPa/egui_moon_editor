use std::ops::Range;

use egui::{Align, Rect, Response, Ui, pos2, vec2};

use crate::{
    style::EditorStyle,
    syntax::{Highlighter, Language, TokenStyle},
    text::char_ranges_to_bytes,
};

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
    /// What the text is read as. Kept beside the highlighter because the highlighter is
    /// thrown away and built again whenever the buffer under it changes.
    language: Language,
    /// How far the grammar has been read down the buffer, and what it found. Only ever asked
    /// about the window on screen, so a file too big to parse in a frame still opens in one.
    highlighter: Highlighter,
}

impl Editor {
    /// An editor over `text`, read as no language in particular until
    /// [`set_language`](Self::set_language) says otherwise.
    pub fn new(text: String) -> Self {
        Self {
            text,
            language: Language::plain(),
            highlighter: Highlighter::new(Language::plain()),
        }
    }

    /// The text as it stands, including whatever has been typed into it.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replace the text, the way loading a file does.
    pub fn set_text(&mut self, text: String) {
        self.text = text;
        // A different buffer entirely, so nothing the highlighter worked out about the old
        // one is worth keeping - not the tokens, and not the parser positions between them.
        self.highlighter = Highlighter::new(self.language.clone());
    }

    /// Read the text as `language` from here on, the way opening a file at a path does.
    pub fn set_language(&mut self, language: Language) {
        self.language = language.clone();
        self.highlighter = Highlighter::new(language);
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
        // The text area's id, settled out here rather than left to egui to derive, because
        // where the caret was *before* the text area ran is what says where an edit happened
        // - and that can only be read back from the state under an id already known.
        let text_id = ui.id().with("moon-editor-text");
        // The text is lent to the `TextEdit` below, so anything worked out from it is worked
        // out here: the layouter is handed the same text back and reads these.
        let byte_marks = char_ranges_to_bytes(&self.text, request.marks.ranges.iter().cloned());
        let line_count = self.text.lines().count().max(1);
        let caret_before = caret_line(ui.ctx(), text_id, &self.text);

        let mut marks_laid_out = 0;
        let mut current_mark_at: Option<Rect> = None;
        let mut line_at: Option<Rect> = None;
        let mut response = None;
        // The text and the highlighter are borrowed apart: the `TextEdit` takes the buffer
        // mutably, and the layouter reads the tokens beside it in the same breath.
        let Self {
            text, highlighter, ..
        } = self;

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

                    // What the vertical scroll area has on screen, in lines, measured off the
                    // top of the content the same way the fringe measures its numbers. The
                    // grammar is read over that window and a screenful either side of it, so
                    // a scroll of less than a page lands on lines already read rather than
                    // racing the parser down the file.
                    let visible = painter.clip_rect().expand(row_height).y_range();
                    let window = lines_across(visible, fringe.min.y, row_height);
                    highlighter.prepare(
                        text,
                        window.start.saturating_sub(rows_on_screen)
                            ..window.end.saturating_add(rows_on_screen),
                    );

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
                                    highlighter,
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
                            let output = egui::TextEdit::multiline(text)
                                .id(text_id)
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
                            let rows = &output.galley.rows;
                            let top = output.galley_pos.y;
                            // The line asked about is what the editor scrolls to, so it is
                            // looked for over the whole galley: the point of asking is usually
                            // that it is not on screen yet.
                            if let Some(wanted) = request.line_of_interest {
                                line_at = rows
                                    .iter()
                                    .zip(line_numbers(rows))
                                    .find(|(_, line)| *line == Some(wanted))
                                    .map(|(placed, _)| {
                                        Rect::from_min_size(
                                            pos2(output.galley_pos.x, top + placed.pos.y),
                                            vec2(1.0, row_height),
                                        )
                                    });
                            }
                            // Rows run down the page in order, so the ones on screen are one
                            // stretch of them: the painting starts where that stretch does and
                            // stops where it ends, rather than walking the file every frame.
                            let first =
                                rows.partition_point(|placed| top + placed.pos.y < visible.min);
                            for (placed, line) in rows.iter().zip(line_numbers(rows)).skip(first) {
                                let Some(line) = line else { continue };
                                let y = top + placed.pos.y;
                                if y > visible.max {
                                    break;
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

        let response = response.expect("the text area is always drawn");
        // A `TextEdit` says *that* the text changed, never where. The caret is where: an edit
        // happens at it, so the line it was on before and the line it is on now bracket
        // everything that moved. The earlier of the two is what has to be re-read - a
        // backspace over a line break and a selection delete both leave the caret above where
        // it started, and a paste of several lines leaves it below - and everything above that
        // line is untouched, because the grammar reaching it never looked further down.
        if response.changed() {
            let caret_after = caret_line(ui.ctx(), text_id, &self.text);
            // With no caret to read there is nothing to say where the edit was, so the whole
            // buffer is suspect. That is only reachable if something changed the text without
            // going through the text area, which is not how this widget is driven.
            let from = match (caret_before, caret_after) {
                (Some(before), Some(after)) => before.min(after),
                (before, after) => before.or(after).unwrap_or(0),
            };
            self.highlighter.invalidate_from(from);
        }

        EditorOutput {
            // The text area is drawn on every path through the closure above, so there is
            // always one to report.
            response,
            marks_laid_out,
            current_mark_at,
            line_at,
        }
    }
}

/// The number of the line each row of a galley starts, counting from one the way the fringe
/// shows them, and nothing for a row that is the rest of a line the row above began.
///
/// A row only starts a line when the row before it ended in a newline, so a number cannot be
/// read off a row's index: it has to be carried down from the top of the galley. Which is why
/// this is an iterator and a caller looking only at the rows on screen still steps over the
/// ones above them - stepping is all it does there, no number is written and nothing is
/// painted.
fn line_numbers(
    rows: &[egui::epaint::text::PlacedRow],
) -> impl Iterator<Item = Option<usize>> + use<'_> {
    let mut line = 0;
    let mut starts_line = true;
    rows.iter().map(move |placed| {
        let starts = starts_line;
        starts_line = placed.ends_with_newline;
        starts.then(|| {
            line += 1;
            line
        })
    })
}

/// The lines a stretch of the screen covers, as an index range from zero, given where the top
/// of the content sits and how tall a line is.
///
/// An estimate, and only ever used to ask the grammar for more than is needed: rows the text
/// area laid out taller than the font's own height would put the answer out by a line or two,
/// which the margin around the window swallows.
fn lines_across(visible: egui::Rangef, top: f32, row_height: f32) -> Range<usize> {
    let line_at = |y: f32| ((y - top) / row_height).max(0.0) as usize;
    line_at(visible.min)..line_at(visible.max) + 1
}

/// The line the caret sits on in `text`, counting from zero, or nothing when the text area
/// has no caret in it.
///
/// The earlier end of a selection, not the caret proper: a selection about to be deleted is
/// edited from its start, wherever the caret sitting in it happens to be.
fn caret_line(ctx: &egui::Context, id: egui::Id, text: &str) -> Option<usize> {
    let state = egui::text_edit::TextEditState::load(ctx, id)?;
    let range = state.cursor.char_range()?;
    let at = range.primary.index.0.min(range.secondary.index.0);
    Some(text.chars().take(at).filter(|c| *c == '\n').count())
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

/// The text laid out: every run in the look its token asks for, every mark tinted behind
/// whatever it covers, and the current mark underlined as well, so stepping between marks is
/// visible without the others disappearing.
///
/// The two are cut against each other - a new run starts at whichever boundary comes first -
/// because a match found by a search lands wherever it lands, usually across the middle of a
/// string or an identifier, and a search should not repaint the code it is searching.
///
/// Marks are byte ranges of `text`. A mark reaching past the end of the text is where the
/// laying out stops: the text can have been edited since the marks were worked out, and the
/// rest of it is drawn plain rather than cut at an offset that is no longer there.
fn marked_text(
    text: &str,
    marks: &[Range<usize>],
    current: usize,
    highlighter: &Highlighter,
    style: &EditorStyle,
    wrap_width: f32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;

    let runs = token_runs(text, highlighter);
    let mut at = 0;
    let mut cut = 0;
    for (index, range) in marks.iter().enumerate() {
        if range.start < cut || range.end > text.len() || !text.is_char_boundary(range.start) {
            break;
        }
        append_runs(
            &mut job,
            text,
            &runs,
            &mut at,
            cut..range.start,
            style,
            MarkLook::none(),
        );
        let mark = MarkLook {
            background: style.mark_ink,
            underline: match index == current {
                true => egui::Stroke::new(1.0, style.current_mark_ink),
                false => egui::Stroke::NONE,
            },
        };
        append_runs(&mut job, text, &runs, &mut at, range.clone(), style, mark);
        cut = range.end;
    }
    let rest = cut..text.len();
    append_runs(
        &mut job,
        text,
        &runs,
        &mut at,
        rest,
        style,
        MarkLook::none(),
    );
    job
}

/// What a mark adds to the runs it covers, on top of the look each one already has.
#[derive(Clone, Copy)]
struct MarkLook {
    background: egui::Color32,
    underline: egui::Stroke,
}

impl MarkLook {
    /// Text no mark reaches, which is most of a page.
    fn none() -> Self {
        Self {
            background: egui::Color32::TRANSPARENT,
            underline: egui::Stroke::NONE,
        }
    }
}

/// Lay `span` of the text into the job, cut where the runs under it change look, with `mark`
/// - a mark's tint, and its underline where it is the current one - laid over each piece.
///
/// `at` is where the last span left off in `runs`; spans arrive in order, so the runs are
/// walked once across the whole text rather than searched for each span.
fn append_runs(
    job: &mut egui::text::LayoutJob,
    text: &str,
    runs: &[(Range<usize>, TokenStyle)],
    at: &mut usize,
    span: Range<usize>,
    style: &EditorStyle,
    mark: MarkLook,
) {
    let mut cut = span.start;
    while cut < span.end {
        // The runs cover the text end to end, so there is one over every byte of every span.
        while runs[*at].0.end <= cut {
            *at += 1;
        }
        let (range, token) = &runs[*at];
        let end = range.end.min(span.end);
        let mut format = code_format(style, *token);
        format.background = mark.background;
        format.underline = mark.underline;
        job.append(&text[cut..end], 0.0, format);
        cut = end;
    }
}

/// The whole text cut into runs of one look each, in order and covering it end to end.
///
/// Tokens are per line and their ranges are relative to the line they are on, so the lines are
/// walked the way the highlighter walks them - inclusive of the newline, which is what makes
/// `"a\n"` one line to both of us - and each line's start is added back on. A line the
/// highlighter has not read yet is one plain run, which is what an editor scrolled faster than
/// the parser shows for a frame.
fn token_runs(text: &str, highlighter: &Highlighter) -> Vec<(Range<usize>, TokenStyle)> {
    let mut runs: Vec<(Range<usize>, TokenStyle)> = Vec::new();
    let mut line_start = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let mut cut = 0;
        for token in highlighter.tokens_on(index) {
            // The buffer can have been edited since these were worked out, in which case the
            // ranges are about a line that is no longer there: the rest of this one is drawn
            // plain rather than cut at an offset that would not be a character boundary.
            if token.range.start != cut
                || token.range.end > line.len()
                || !line.is_char_boundary(token.range.end)
            {
                break;
            }
            push_run(
                &mut runs,
                line_start + token.range.start..line_start + token.range.end,
                token.style,
            );
            cut = token.range.end;
        }
        // Whatever the tokens did not cover, the newline at the end of the line included.
        push_run(
            &mut runs,
            line_start + cut..line_start + line.len(),
            TokenStyle::Plain,
        );
        line_start += line.len();
    }
    runs
}

/// Add a run, unless it is empty, joining it to the one before where they look the same: two
/// runs of one look are one run to lay out.
fn push_run(runs: &mut Vec<(Range<usize>, TokenStyle)>, range: Range<usize>, style: TokenStyle) {
    if range.is_empty() {
        return;
    }
    match runs.last_mut() {
        Some((last, look)) if *look == style && last.end == range.start => last.end = range.end,
        _ => runs.push((range, style)),
    }
}

/// One run of the editor's text, in the look the style gives that kind of token.
fn code_format(style: &EditorStyle, token: TokenStyle) -> egui::TextFormat {
    let look = style.syntax.look(token);
    egui::TextFormat {
        font_id: look.font.clone(),
        color: look.ink,
        italics: look.italics,
        ..Default::default()
    }
}
