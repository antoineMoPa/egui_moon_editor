use std::ops::Range;

use egui::{Align, Rect, Response, Ui, pos2, vec2};

use crate::{
    changes::NewLines,
    completing,
    completing::{Completion, Listing},
    indenting::{self, Indent},
    place::{
        TextPoint, Word, byte_of_char, caret_at, chars_before, text_point, word_around, word_at,
        word_before, word_still_at,
    },
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
    /// The modifier that turns the word under the pointer into something to click, the way a
    /// browser makes a link of it. `None` — the default — is an editor with nothing to
    /// navigate to, which is every editor until the caller has somewhere to send it.
    ///
    /// The caller picks the modifier because which one it is is a platform convention —
    /// command on macOS, ctrl everywhere else — and a widget has no business holding an
    /// opinion about the platform it was dropped into.
    pub navigate_modifier: Option<egui::Modifiers>,
    /// What to offer under the caret. Empty offers nothing, which is the usual state.
    pub completions: &'a [Completion],
    /// What a Tab press puts in. Four spaces until the caller says otherwise, because how a
    /// file is indented is a fact about the repo it is in and the caller is what knows the
    /// repo — see [`Indent`].
    pub indent: Indent,
    /// Stretches of the text to underline in a colour of the caller's - what a language server
    /// found wrong, say. One that no longer fits the text, typed into since the ranges were
    /// worked out, is left out for the frame rather than cut where there is no character
    /// boundary. A stretch already underlined - the current mark, the word under a held
    /// modifier - keeps its own underline.
    pub underlines: &'a [Underline],
}

/// A stretch of the text to underline, and in what colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Underline {
    /// Bytes of the text as it stands.
    pub range: Range<usize>,
    /// The colour of the line under it.
    pub color: egui::Color32,
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
    /// The word under the pointer while the navigate modifier is held — underlined and shown
    /// with a pointing-hand cursor, so it reads as clickable before it is clicked.
    pub navigable_word: Option<Word>,
    /// The word clicked with that modifier held, on the frame it was clicked.
    pub navigated_to: Option<Word>,
    /// Where the caret sits, when the text area has one.
    pub caret: Option<TextPoint>,
    /// The item taken this frame, by Enter, Tab or a click. Already in the text — reported so
    /// the caller can stop offering and, if it wants, say what it did.
    pub completion_taken: Option<Completion>,
    /// Whether the list was put away with Escape this frame, so the caller can stop offering
    /// until something makes it worth offering again.
    pub completion_dismissed: bool,
    /// The word being typed at the caret, when there is one: what the caller works out its
    /// candidates from. `None` when the caret is not at the end of a word.
    pub word_being_typed: Option<Word>,
    /// The word the caret sits in, when it sits in one: what a caller asks about a name from
    /// the keyboard or a menu, the way [`navigated_to`](Self::navigated_to) is from a click.
    pub word_at_caret: Option<Word>,
    /// Where in the text the pointer is, while it is over the text: what a caller showing
    /// something about the place under the pointer - a diagnostic's message - reads.
    pub pointed_at: Option<TextPoint>,
    /// The word under the pointer, while there is one: what a caller showing something about
    /// the name under the pointer - its type, its docs - asks about. The word
    /// [`navigable_word`](Self::navigable_word) is, without a modifier held.
    pub pointed_word: Option<Word>,
    /// Where the caret is on screen, when the text area has one: what a caller hangs a popup
    /// about the place being typed at off - the signature of a call.
    pub caret_rect: Option<Rect>,
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
    /// The word the pointer was over last frame, kept so this frame can underline it.
    ///
    /// The chicken and the egg: the word under the pointer is found by hit-testing the
    /// laid-out text, and laying the text out is the same call that would have to know about
    /// the underline. So the underline is always a frame behind the pointer - invisible at
    /// any speed a pointer moves, and much cheaper than laying the text out twice.
    hovered_word: Option<Word>,
    /// The list of things to finish the word being typed with: which row is current, and
    /// whether it has been put away. The caller says what is in the list; which row the
    /// keyboard is on is the editor's, since the editor is what draws the rows.
    completing: Listing,
    /// The lines of the text that the text it is compared against does not have, drawn as a
    /// bar down the right of the fringe - and when to look again after the text is typed into.
    new_lines: NewLines,
    /// Edits put into the text from outside since the text area last ran - see
    /// [`replace_ranges`](Self::replace_ranges). The caret the text area is holding is a place
    /// in the text as it was before them, and it is carried through them the next time the
    /// editor is drawn, which is the first moment the text area's state can be reached.
    outside_edits: Vec<OutsideEdit>,
}

/// A stretch of the text replaced from outside, counted in characters of the text as it was
/// before - the unit egui's cursor counts in, and the text its cursor was a place in.
struct CharEdit {
    start: usize,
    end: usize,
    /// How many characters were put in its place.
    inserted: usize,
}

/// One call's worth of outside edits, waiting for the frame that carries the caret through them.
struct OutsideEdit {
    edits: Vec<CharEdit>,
    /// The first line they touched, which is where the fringe's bars start moving.
    from_line: usize,
}

/// Where a place in the text lands once the edits have gone in. A place before an edit stays
/// where it is, one after it moves by what the edit added or took away, and one inside the
/// text an edit replaced ends up at the end of what replaced it - a caret in the middle of a
/// name that was renamed is left at the end of the new one.
fn carried_through(edits: &[CharEdit], place: usize) -> usize {
    let mut moved_by: isize = 0;
    for edit in edits {
        if edit.end <= place {
            moved_by += edit.inserted as isize - (edit.end - edit.start) as isize;
            continue;
        }
        if edit.start < place {
            return (edit.start as isize + moved_by) as usize + edit.inserted;
        }
        break;
    }
    (place as isize + moved_by) as usize
}

impl Editor {
    /// An editor over `text`, read as no language in particular until
    /// [`set_language`](Self::set_language) says otherwise.
    pub fn new(text: String) -> Self {
        Self {
            text,
            language: Language::plain(),
            highlighter: Highlighter::new(Language::plain()),
            hovered_word: None,
            completing: Listing::default(),
            new_lines: NewLines::default(),
            outside_edits: Vec::new(),
        }
    }

    /// Replace stretches of the text with other text, the way a rename across a project does:
    /// the rest of the buffer, and the caret and selection in it, stay where they were.
    ///
    /// [`set_text`](Self::set_text) is the other way to change the text from outside, and it
    /// is for a different text altogether - a file loaded in place of another. This is the
    /// same text with some of it changed, so the caret is carried through the change rather
    /// than left at a character count that now means somewhere else, and the text area's own
    /// undo takes the change back like any other.
    ///
    /// `edits` are byte ranges of the text as it stands, in order and never overlapping. That
    /// is the caller's contract, and a list that breaks it panics rather than being put in some
    /// order that would land an edit where it was not meant.
    pub fn replace_ranges(&mut self, edits: Vec<(Range<usize>, String)>) {
        let Some((first, _)) = edits.first() else {
            return;
        };
        assert!(
            edits
                .windows(2)
                .all(|pair| pair[0].0.end <= pair[1].0.start),
            "the edits have to be in order and not overlap"
        );
        let from_line = self.text[..first.start].matches('\n').count();

        // In characters, counted forwards once rather than from the start of the text for each
        // edit: a rename in a long file is many edits, and they are all in order.
        let mut moves = Vec::with_capacity(edits.len());
        let mut chars = 0;
        let mut counted_to = 0;
        for (range, with) in &edits {
            chars += self.text[counted_to..range.start].chars().count();
            let start = chars;
            chars += self.text[range.clone()].chars().count();
            counted_to = range.end;
            moves.push(CharEdit {
                start,
                end: chars,
                inserted: with.chars().count(),
            });
        }
        for (range, with) in edits.into_iter().rev() {
            self.text.replace_range(range, &with);
        }

        self.highlighter.invalidate_from(from_line);
        // The word the pointer was over and the list that was up are both of the text before.
        self.hovered_word = None;
        self.completing.dismiss();
        self.outside_edits.push(OutsideEdit {
            edits: moves,
            from_line,
        });
    }

    /// Carry the text area's caret through whatever was put into the text from outside since it
    /// last ran, and move the fringe's bars with their lines.
    fn carry_the_caret_through_outside_edits(&mut self, ui: &Ui, text_id: egui::Id) {
        for outside in std::mem::take(&mut self.outside_edits) {
            // A text area that has never been drawn has no caret to carry.
            if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), text_id)
                && let Some(mut range) = state.cursor.char_range()
            {
                range.primary = egui::text::CCursor::new(carried_through(
                    &outside.edits,
                    range.primary.index.0,
                ));
                range.secondary = egui::text::CCursor::new(carried_through(
                    &outside.edits,
                    range.secondary.index.0,
                ));
                state.cursor.set_char_range(Some(range));
                state.store(ui.ctx(), text_id);
            }
            let caret = caret_at(ui.ctx(), text_id, &self.text).map(|point| point.line);
            let now = ui.input(|input| input.time);
            self.new_lines
                .edited(&self.text, outside.from_line, caret, now);
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
        // one is worth keeping - not the tokens, and not the parser positions between them,
        // and not the word the pointer was over, which was a word of the text that is gone.
        self.highlighter = Highlighter::new(self.language.clone());
        self.hovered_word = None;
        self.new_lines.text_replaced(&self.text);
    }

    /// Compare the text against `base` from here on, and mark the lines of it that `base`
    /// does not have with a bar down the right of the fringe - the file as it was last
    /// committed, say, so what has been written since stands out. `None` marks nothing.
    ///
    /// Typing is only compared once the text has been left alone for a few seconds, and a bar
    /// that turns up then fades in, so the fringe does not flicker along with every keystroke.
    pub fn set_base(&mut self, base: Option<String>) {
        self.new_lines.set_base(base, &self.text);
    }

    /// The lines the fringe marks as new, as ranges of line indexes counted from zero, in
    /// order - which, while the text is being typed into, is what it marked before the typing
    /// began. Empty with no base.
    pub fn new_lines(&self) -> &[Range<usize>] {
        self.new_lines.lines()
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
        // Before anything reads the caret: it is still a place in the text as it was before
        // whatever was put in from outside.
        self.carry_the_caret_through_outside_edits(ui, text_id);
        // The list of things to finish the word being typed with, settled before anything is
        // drawn: whether it is on screen is what says who gets the arrows, Enter, Tab and
        // Escape this frame, and the text area reads those the moment it runs.
        self.completing.offered(request.completions);
        let focused = request.focus || ui.memory(|memory| memory.has_focus(text_id));
        let mut presses = completing::Presses::default();
        if self.completing.showing(request.completions, focused) {
            presses = completing::take_keys(ui, &mut self.completing, request.completions.len());
        }
        // Drawn only while it is still the answer to something: a list that was just taken
        // from or put away is already gone by the time this frame is painted.
        let drawing_list = self.completing.showing(request.completions, focused)
            && presses.take.is_none()
            && !presses.dismissed;

        // Tab is one level of indentation, taken out of the events here and put into the text
        // before the text area runs - which is also what stops egui typing a `\t`. Not while
        // a list is on screen: there Tab takes the row under the keyboard, and `take_keys`
        // above has already had it, and not while the keyboard is elsewhere, where a tab is
        // egui moving the focus along.
        let indented = match focused && !drawing_list {
            true => indenting::take_tabs(ui, text_id, &mut self.text, request.indent),
            false => None,
        };
        if let Some(line) = indented {
            self.highlighter.invalidate_from(line);
        }

        // The text is lent to the `TextEdit` below, so anything worked out from it is worked
        // out here: the layouter is handed the same text back and reads these.
        let byte_marks = char_ranges_to_bytes(&self.text, request.marks.ranges.iter().cloned());
        let line_count = self.text.lines().count().max(1);
        let caret_before = caret_at(ui.ctx(), text_id, &self.text).map(|point| point.line);
        // Held now, rather than asked about at the moment of the click: the underline and the
        // pointing hand have to be there before the click, which is the whole point of them.
        let navigating = request
            .navigate_modifier
            .is_some_and(|wanted| ui.input(|input| input.modifiers.matches_exact(wanted)));
        // What to underline, from where the pointer was last frame - and only while the text
        // under it is still the word that was found there, since the buffer can have been
        // typed into in between.
        let underline = match navigating {
            true => self
                .hovered_word
                .as_ref()
                .and_then(|word| word_still_at(&self.text, word)),
            false => None,
        };

        let mut marks_laid_out = 0;
        let mut clicked_completion: Option<usize> = None;
        let mut current_mark_at: Option<Rect> = None;
        let mut line_at: Option<Rect> = None;
        let mut response = None;
        let mut navigable_word: Option<Word> = None;
        let mut caret_rect: Option<Rect> = None;
        let mut pointed_word: Option<Word> = None;
        let mut pointed_at: Option<TextPoint> = None;
        let mut navigated_to: Option<Word> = None;
        // The text and the highlighter are borrowed apart: the `TextEdit` takes the buffer
        // mutably, and the layouter reads the tokens beside it in the same breath.
        let Self {
            text,
            highlighter,
            completing: listing,
            new_lines,
            ..
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
                                    underline.clone(),
                                    request.underlines,
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
                            // A right-click puts the caret where it was made, the way a left
                            // one does: a menu opened on a name is about that name, and what a
                            // caller asks about is the place the caret is at. Not inside a
                            // selection, which is what a menu opened over it is about.
                            if output.response.secondary_clicked()
                                && let Some(pointer) =
                                    ui.input(|input| input.pointer.interact_pos())
                            {
                                let clicked =
                                    output.galley.cursor_from_pos(pointer - output.galley_pos);
                                let in_the_selection = output.cursor_range.is_some_and(|range| {
                                    let [min, max] = range.sorted_cursors();
                                    min.index < max.index
                                        && (min.index..=max.index).contains(&clicked.index)
                                });
                                if !in_the_selection {
                                    let mut state = output.state.clone();
                                    state.cursor.set_char_range(Some(
                                        egui::text::CCursorRange::one(clicked),
                                    ));
                                    state.store(ui.ctx(), text_id);
                                    output.response.request_focus();
                                }
                            }
                            // An edit moves the bars below it with their lines; the comparison
                            // itself waits for the typing to stop, and is told where the caret
                            // was left, which is where the typing was. The edit starts at the
                            // earliest line anything happened on.
                            let now = ui.input(|input| input.time);
                            if output.response.changed() || indented.is_some() {
                                let caret = caret_at(ui.ctx(), text_id, text.as_str())
                                    .map(|point| point.line);
                                let at = [caret_before, caret, indented]
                                    .into_iter()
                                    .flatten()
                                    .min()
                                    .unwrap_or(0);
                                new_lines.edited(text.as_str(), at, caret, now);
                            }
                            // Nothing else may draw a frame once the typing stops, so the
                            // fringe asks for the one its comparison is due on, and for every
                            // frame of a fade.
                            if let Some(after) = new_lines.settle(text.as_str(), now) {
                                ui.ctx().request_repaint_after(after);
                            }

                            // The word under the pointer, found in the text as it was just
                            // laid out - so what is reported is this frame's, and only the
                            // underline lags. `contains_pointer` rather than `hovered`: the
                            // answer is about where the pointer is, not about whether the
                            // text area is the widget entitled to react to it.
                            if output.response.contains_pointer()
                                && let Some(pointer) =
                                    ui.input(|input| input.pointer.interact_pos())
                            {
                                pointed_word = word_at(
                                    text.as_str(),
                                    &output.galley,
                                    output.galley_pos,
                                    pointer,
                                );
                                let under =
                                    output.galley.cursor_from_pos(pointer - output.galley_pos);
                                pointed_at = Some(text_point(
                                    text.as_str(),
                                    byte_of_char(text.as_str(), under.index.0),
                                ));
                            }
                            if navigating {
                                navigable_word = pointed_word.clone();
                            }
                            if let Some(word) = &navigable_word {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                if output.response.clicked() {
                                    navigated_to = Some(word.clone());
                                }
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
                                    .zip(row_lines(rows))
                                    .find(|(_, line)| line.starts && line.number == wanted)
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
                            for (placed, line) in rows.iter().zip(row_lines(rows)).skip(first) {
                                let y = top + placed.pos.y;
                                if y > visible.max {
                                    break;
                                }
                                // Every row of a new line, the rest of one that wraps included,
                                // so the bar runs the height of what is new.
                                if let Some(opacity) = new_lines.opacity(line.number - 1, now) {
                                    painter.rect_filled(
                                        Rect::from_min_max(
                                            pos2(fringe.max.x - style.new_line_bar_width, y),
                                            pos2(fringe.max.x, y + placed.rect().height()),
                                        ),
                                        0.0,
                                        style.new_line_ink.gamma_multiply(opacity),
                                    );
                                }
                                if !line.starts {
                                    continue;
                                }
                                painter.text(
                                    pos2(fringe.max.x - 6.0, y),
                                    egui::Align2::RIGHT_TOP,
                                    line.number.to_string(),
                                    style.line_number_font.clone(),
                                    style.fringe_ink,
                                );
                            }

                            // Where the caret is on screen, for a caller hanging something off
                            // it - the signature of the call being typed.
                            if let Some(caret) = caret_at(ui.ctx(), text_id, text.as_str()) {
                                let at = egui::text::CCursor::new(chars_before(
                                    text.as_str(),
                                    caret.offset,
                                ));
                                caret_rect = Some(
                                    output
                                        .galley
                                        .pos_from_cursor(at)
                                        .translate(output.galley_pos.to_vec2()),
                                );
                            }
                            // Hung off the caret in the text as it was just laid out, which is
                            // the only place the caret's rect can be measured from.
                            if drawing_list
                                && let Some(caret) = caret_at(ui.ctx(), text_id, text.as_str())
                            {
                                let at = egui::text::CCursor::new(chars_before(
                                    text.as_str(),
                                    caret.offset,
                                ));
                                let caret_at = output
                                    .galley
                                    .pos_from_cursor(at)
                                    .translate(output.galley_pos.to_vec2());
                                clicked_completion = completing::draw(
                                    ui,
                                    style,
                                    request.completions,
                                    listing,
                                    caret_at,
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
        // Kept for the next frame to underline: this frame's layout is already behind us.
        self.hovered_word = navigable_word.clone();
        // A `TextEdit` says *that* the text changed, never where. The caret is where: an edit
        // happens at it, so the line it was on before and the line it is on now bracket
        // everything that moved. The earlier of the two is what has to be re-read - a
        // backspace over a line break and a selection delete both leave the caret above where
        // it started, and a paste of several lines leaves it below - and everything above that
        // line is untouched, because the grammar reaching it never looked further down.
        if response.changed() {
            let caret_after = caret_at(ui.ctx(), text_id, &self.text).map(|point| point.line);
            // With no caret to read there is nothing to say where the edit was, so the whole
            // buffer is suspect. That is only reachable if something changed the text without
            // going through the text area, which is not how this widget is driven.
            let from = match (caret_before, caret_after) {
                (Some(before), Some(after)) => before.min(after),
                (before, after) => before.or(after).unwrap_or(0),
            };
            self.highlighter.invalidate_from(from);
        }

        // Put in after the text area has run rather than before it: the caret it is measured
        // from is the one the text area just stored, so a character typed in the same frame as
        // the Enter that took a row is already accounted for.
        let completion_taken = presses
            .take
            .or(clicked_completion)
            .and_then(|index| request.completions.get(index))
            .cloned();
        if let Some(completion) = &completion_taken {
            let from = insert_completion(ui.ctx(), text_id, &mut self.text, completion);
            self.highlighter.invalidate_from(from);
            let caret = caret_at(ui.ctx(), text_id, &self.text).map(|point| point.line);
            let now = ui.input(|input| input.time);
            self.new_lines.edited(&self.text, from, caret, now);
            self.completing.dismiss();
        }

        // Left behind for the next frame, after the text area has set a filter of its own:
        // while there is a list on screen, Escape and Tab are the list's, and egui works out
        // whose they are before any of this runs.
        if self
            .completing
            .showing(request.completions, response.has_focus())
        {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(text_id, completing::LIST_KEEPS_KEYS);
            });
        }

        // Read after everything that could have moved it: what was typed, and a row taken.
        let caret = caret_at(ui.ctx(), text_id, &self.text);
        let word_being_typed = caret
            .as_ref()
            .and_then(|point| word_before(&self.text, point.offset));
        let word_at_caret = caret.as_ref().and_then(|point| {
            let range = word_around(&self.text, point.offset)?;
            Some(Word {
                text: self.text[range.clone()].to_string(),
                at: text_point(&self.text, range.start),
            })
        });

        EditorOutput {
            // The text area is drawn on every path through the closure above, so there is
            // always one to report.
            response,
            marks_laid_out,
            current_mark_at,
            line_at,
            navigable_word,
            navigated_to,
            // Read after the text area has run, so an edit or a click this frame is already
            // in it: where the caret is now is what a caller showing a line and column, or
            // asking a server about the place it sits, has to be told.
            caret,
            completion_taken,
            completion_dismissed: presses.dismissed,
            word_being_typed,
            word_at_caret,
            pointed_at,
            pointed_word,
            caret_rect,
        }
    }
}

/// Put a taken candidate into the text in place of the word being typed at the caret, leave
/// the caret where the candidate asked for it, and say which line the change starts on.
///
/// The word is found around the caret here rather than carried over from the frame the
/// candidates were worked out on: the buffer can have been typed into in between, and what is
/// replaced has to be what is under the caret now. A caret on a space replaces nothing and the
/// text is put in where it sits.
///
/// Where the caret is left is the end of the insertion less the candidate's
/// [`Completion::caret_back`], which for a candidate that put a call in is inside the
/// parentheses it just wrote. The distance is the caller's, and this is the only thing the
/// editor does about it.
///
/// The cursor is stored back into the text area's own state, after the fact, because the text
/// area has already run this frame and is holding a cursor into the text as it was before.
fn insert_completion(
    ctx: &egui::Context,
    id: egui::Id,
    text: &mut String,
    completion: &Completion,
) -> usize {
    let insert = &completion.insert;
    let at = caret_at(ctx, id, text).map_or(text.len(), |point| point.offset);
    let range = word_around(text, at).unwrap_or(at..at);
    let line = text_point(text, range.start).line;
    text.replace_range(range.clone(), insert);

    let leave_at = range.start
        + insert
            .len()
            .checked_sub(completion.caret_back)
            .expect("a caret left further back than the insertion is long is nowhere");
    let end = chars_before(text, leave_at);
    let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(end),
        )));
    state.store(ctx, id);
    line
}

/// The line a row of a galley is part of.
#[derive(Clone, Copy)]
struct RowLine {
    /// Counting from one, the way the fringe shows it.
    number: usize,
    /// Whether the row is where the line starts, rather than the rest of a line the row above
    /// began. Only such a row gets a number in the fringe.
    starts: bool,
}

/// The line each row of a galley is part of.
///
/// A row only starts a line when the row before it ended in a newline, so a number cannot be
/// read off a row's index: it has to be carried down from the top of the galley. Which is why
/// this is an iterator and a caller looking only at the rows on screen still steps over the
/// ones above them - stepping is all it does there, no number is written and nothing is
/// painted.
fn row_lines(rows: &[egui::epaint::text::PlacedRow]) -> impl Iterator<Item = RowLine> + use<'_> {
    let mut number = 0;
    let mut starts_line = true;
    rows.iter().map(move |placed| {
        let starts = starts_line;
        starts_line = placed.ends_with_newline;
        if starts {
            number += 1;
        }
        RowLine { number, starts }
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
    navigable: Option<Range<usize>>,
    underlines: &[Underline],
    highlighter: &Highlighter,
    style: &EditorStyle,
    wrap_width: f32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;

    let runs = token_runs(text, highlighter);
    let mut at = 0;
    for (span, look) in marked_spans(text, marks, current, navigable, underlines, style) {
        append_runs(&mut job, text, &runs, &mut at, span, style, look);
    }
    job
}

/// The text cut into spans of one [`MarkLook`] each, in order and covering it end to end.
///
/// Two things are laid over the text here and they are cut against each other rather than one
/// winning: a search can find a match inside the word the pointer is over, and neither the
/// match nor the word it is in should disappear because of the other.
fn marked_spans(
    text: &str,
    marks: &[Range<usize>],
    current: usize,
    navigable: Option<Range<usize>>,
    underlines: &[Underline],
    style: &EditorStyle,
) -> Vec<(Range<usize>, MarkLook)> {
    let mut spans = Vec::new();
    let mut cut = 0;
    for (index, range) in marks.iter().enumerate() {
        if range.start < cut || range.end > text.len() || !text.is_char_boundary(range.start) {
            break;
        }
        let mark = MarkLook {
            background: style.mark_ink,
            underline: match index == current {
                true => egui::Stroke::new(1.0, style.current_mark_ink),
                false => egui::Stroke::NONE,
            },
        };
        spans.push((cut..range.start, MarkLook::none()));
        spans.push((range.clone(), mark));
        cut = range.end;
    }
    spans.push((cut..text.len(), MarkLook::none()));

    // The word was found in the text as it was at the top of the frame, and a `TextEdit` lays
    // its text out again after applying what was typed into it - so by here the word can be
    // over an offset that is no longer a character boundary, and cutting the text there would
    // panic. A frame with no underline on it is the right answer to that. The caller's
    // underlines are the same: worked out against the text as it was.
    if let Some(word) = navigable.filter(|word| fits(text, word)) {
        spans = spans
            .into_iter()
            .flat_map(|(span, look)| underlined_word(span, look, &word, style))
            .collect();
    }
    for underline in underlines
        .iter()
        .filter(|underline| fits(text, &underline.range))
    {
        spans = spans
            .into_iter()
            .flat_map(|(span, look)| underlined_in_colour(span, look, underline))
            .collect();
    }
    spans
}

/// Whether a range can be cut out of the text as it stands.
fn fits(text: &str, range: &Range<usize>) -> bool {
    range.start <= range.end
        && range.end <= text.len()
        && text.is_char_boundary(range.start)
        && text.is_char_boundary(range.end)
}

/// One span cut where a caller's underline starts and ends, with the piece inside it underlined
/// in the caller's colour - unless the span is underlined already, which keeps what it has.
fn underlined_in_colour(
    span: Range<usize>,
    look: MarkLook,
    underline: &Underline,
) -> Vec<(Range<usize>, MarkLook)> {
    let inside = span.start.max(underline.range.start)..span.end.min(underline.range.end);
    if inside.is_empty() || look.underline != egui::Stroke::NONE {
        return vec![(span, look)];
    }
    let underlined = MarkLook {
        underline: egui::Stroke::new(1.5, underline.color),
        ..look
    };
    vec![
        (span.start..inside.start, look),
        (inside.clone(), underlined),
        (inside.end..span.end, look),
    ]
}

/// One span cut where the word under the pointer starts and ends, with the piece inside it
/// underlined: the word reads as a link, in the ink the text around it is set in, so it is the
/// underline that says it can be clicked rather than a colour the code does not otherwise use.
fn underlined_word(
    span: Range<usize>,
    look: MarkLook,
    word: &Range<usize>,
    style: &EditorStyle,
) -> Vec<(Range<usize>, MarkLook)> {
    let inside = span.start.max(word.start)..span.end.min(word.end);
    if inside.is_empty() {
        return vec![(span, look)];
    }
    let underlined = MarkLook {
        underline: egui::Stroke::new(1.0, style.ink),
        ..look
    };
    vec![
        (span.start..inside.start, look),
        (inside.clone(), underlined),
        (inside.end..span.end, look),
    ]
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
