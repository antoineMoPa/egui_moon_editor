//! Finishing a word as it is typed: a list under the caret that the caller fills and the
//! editor draws.
//!
//! The split is the same one the rest of the widget is built on. Working out what could finish
//! `val` is the caller's job — it is the half that knows what the text means — and putting the
//! chosen one into the buffer is the editor's, because the editor is what owns the buffer and
//! where the caret is in it. So the caller hands over a list of things and is told which one
//! was taken; it never touches the text.
//!
//! The awkward part is the keyboard. A list on screen wants the arrows, Enter, Tab and Escape,
//! and the text under it wants exactly the same keys. They cannot both have them, so while the
//! list is showing the editor lifts those presses out of the input queue before the text area
//! ever sees them — and only those, so everything else, typing included, still lands in the
//! text.

use egui::{Align2, CornerRadius, Key, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::style::EditorStyle;

/// One thing the caller is offering to finish the word being typed with.
///
/// Where these come from is the caller's business — a language's grammar, a list of the names
/// already in the file, a dictionary. The editor draws them and puts the chosen one in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// What the row reads as.
    pub label: String,
    /// The quieter half of the row: a type, a signature, where it came from. `None` for a row
    /// that is only its label.
    pub detail: Option<String>,
    /// What goes into the text when this one is taken, in place of the word being typed.
    pub insert: String,
}

/// What the editor keeps about the list between frames: which row is current, and whether the
/// list has been put away.
///
/// It lives here rather than in the caller because the editor is what draws the rows: a caller
/// holding the highlight would have to be told the row height, the number that fit and which
/// way the list opened, all to answer a question the editor already knows the answer to.
#[derive(Default)]
pub(crate) struct Listing {
    /// What was offered when the highlight was last settled. A different list is a different
    /// question, so the highlight goes back to the top of it — the same rule the rest of this
    /// product's lists are written to.
    offered: Vec<String>,
    /// Which row is current, counting from zero.
    selected: usize,
    /// Whether Escape, or a taken row, has put this list away. Sticky until the caller offers
    /// something else, so a caller that keeps offering the same list after an Escape does not
    /// put the popup straight back up.
    dismissed: bool,
}

impl Listing {
    /// Take in what is being offered this frame, resetting the highlight when the offer has
    /// changed and keeping it in range when the list has only got shorter.
    pub(crate) fn offered(&mut self, completions: &[Completion]) {
        let labels: Vec<String> = completions.iter().map(|item| item.label.clone()).collect();
        if labels != self.offered {
            self.offered = labels;
            self.selected = 0;
            self.dismissed = false;
        }
        self.selected = self.selected.min(completions.len().saturating_sub(1));
    }

    /// Whether there is a list on screen this frame: something offered, not put away, and the
    /// editor holding the keyboard — a popup left over a pane that has moved on is the thing
    /// to avoid.
    pub(crate) fn showing(&self, completions: &[Completion], focused: bool) -> bool {
        focused && !self.dismissed && !completions.is_empty()
    }

    /// Put the list away: Escape pressed, or a row taken.
    pub(crate) fn dismiss(&mut self) {
        self.dismissed = true;
    }

    /// Move the highlight, stopping at either end rather than wrapping, which is how every
    /// other list in this product moves.
    fn step(&mut self, by: Step, len: usize) {
        let last = len.saturating_sub(1);
        self.selected = match by {
            Step::Down => (self.selected + 1).min(last),
            Step::Up => self.selected.saturating_sub(1),
        };
    }
}

/// Which way a press moves the highlight.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    Up,
    Down,
}

/// What one of the keys the list answers means.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Press {
    /// Move the highlight.
    Move(Step),
    /// Put the highlighted row into the text.
    Take,
    /// Put the list away.
    Dismiss,
}

/// The keys the list answers while it is showing, and what each one means.
///
/// A table rather than a chain of branches because it is the whole contract with the text
/// underneath: every key in this column is one the text area does not get to see this frame,
/// and every key that is not in it reaches the text untouched. Page Up, Page Down, Home and
/// End are deliberately absent — those move through a file, and a file is what is behind the
/// list rather than what is in it.
const LIST_KEYS: &[(Key, Press)] = &[
    (Key::ArrowDown, Press::Move(Step::Down)),
    (Key::ArrowUp, Press::Move(Step::Up)),
    (Key::Enter, Press::Take),
    (Key::Tab, Press::Take),
    (Key::Escape, Press::Dismiss),
];

/// The keys a showing list needs egui itself to keep its hands off.
///
/// Lifting a press out of this frame's events is not enough for two of them. egui settles where
/// the keyboard goes next from the presses at the top of the frame, before any widget has run:
/// a Tab moves the focus along and an Escape drops it altogether, whatever the events say by
/// the time the text area reads them. Which widget those two belong to is decided by a filter
/// the focused widget leaves behind for the next frame, and this is the filter to leave while
/// there is a list on screen — otherwise the Escape that puts the list away also takes the
/// keyboard off the text underneath it.
pub(crate) const LIST_KEEPS_KEYS: egui::EventFilter = egui::EventFilter {
    tab: true,
    horizontal_arrows: true,
    vertical_arrows: true,
    escape: true,
};

/// What the key means to the list, or nothing when the list has no use for it.
fn press_of(key: Key) -> Option<Press> {
    LIST_KEYS
        .iter()
        .find(|(named, _)| *named == key)
        .map(|(_, press)| *press)
}

/// Lift the list's keys out of this frame's events and act on them, before the text area has
/// had a chance to read them.
///
/// Both halves of a press go — the key going down and the key coming back up — so a text area
/// that watches for the release of a key it never saw pressed is not left holding half of one.
/// Everything else stays in the queue in the order it arrived.
///
/// Answers with what the presses came to.
pub(crate) fn take_keys(ui: &Ui, listing: &mut Listing, len: usize) -> Presses {
    let presses: Vec<Press> = ui.input_mut(|input| {
        let mut taken = Vec::new();
        input.events.retain(|event| {
            let egui::Event::Key { key, pressed, .. } = event else {
                return true;
            };
            let Some(press) = press_of(*key) else {
                return true;
            };
            if *pressed {
                taken.push(press);
            }
            false
        });
        taken
    });

    let mut answer = Presses::default();
    for press in presses {
        match press {
            Press::Move(step) => listing.step(step, len),
            Press::Take => answer.take = Some(listing.selected),
            Press::Dismiss => {
                listing.dismiss();
                return Presses {
                    take: None,
                    dismissed: true,
                };
            }
        }
    }
    answer
}

/// What the keys lifted out of a frame came to.
#[derive(Default)]
pub(crate) struct Presses {
    /// The row Enter or Tab asked for, when one of them was pressed.
    pub(crate) take: Option<usize>,
    /// Whether Escape put the list away this frame.
    pub(crate) dismissed: bool,
}

/// Draw the list under — or over — the caret, and say which row was clicked.
///
/// The rows are set in the editor's own faces on a panel of the surrounding theme, so the list
/// reads as part of the page it is finishing a word on rather than as a window that landed on
/// top of it.
pub(crate) fn draw(
    ui: &Ui,
    style: &EditorStyle,
    completions: &[Completion],
    listing: &mut Listing,
    caret: Rect,
) -> Option<usize> {
    let row_height = ui.fonts_mut(|fonts| fonts.row_height(&style.font)) + style.completion_row_pad;
    // The rows that fit, and which stretch of the list they show: the highlight is kept on
    // screen by scrolling the window down to it, so arrowing past the bottom row moves the
    // list rather than the highlight off it.
    let shown = completions.len().min(style.completion_rows);
    let first = listing
        .selected
        .saturating_sub(shown.saturating_sub(1))
        .min(completions.len() - shown);
    let height = row_height * shown as f32 + style.completion_margin.sum().y;

    let screen = ui.ctx().viewport_rect();
    // Below the caret is where a list of things to finish a word with belongs: it reads as
    // following on from what is being typed, and it does not cover the line above. It only
    // goes over the caret when the rows would otherwise run off the bottom of the screen and
    // there is more room above than below - a list drawn half off the screen hides exactly the
    // rows that matter, since the first of them is the one Enter would take.
    let below = caret.bottom() + style.completion_gap;
    let above = caret.top() - style.completion_gap - height;
    let room_below = screen.bottom() - below;
    let top = match room_below < height && above > screen.top() {
        true => above,
        false => below,
    };

    let mut clicked = None;
    egui::Area::new(ui.id().with("moon-editor-completions"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(caret.left(), top))
        .constrain(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(style.completion_ground)
                .stroke(Stroke::new(1.0, style.completion_edge))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(style.completion_margin)
                .show(ui, |ui| {
                    ui.set_width(style.completion_width);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let on_screen = completions.iter().enumerate().skip(first).take(shown);
                    for (index, completion) in on_screen {
                        let row = draw_row(
                            ui,
                            style,
                            completion,
                            index == listing.selected,
                            row_height,
                        );
                        if row.clicked() {
                            clicked = Some(index);
                        }
                        // The pointer moving over a row makes it the current one, so a click
                        // takes what was under the pointer whichever half of it lands first.
                        if row.hovered() {
                            listing.selected = index;
                        }
                    }
                });
        });
    clicked
}

/// One row: what it reads as on the left, and the quieter half of it against the right edge.
fn draw_row(
    ui: &mut Ui,
    style: &EditorStyle,
    completion: &Completion,
    current: bool,
    row_height: f32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter();
    if current {
        painter.rect_filled(rect, CornerRadius::same(3), style.completion_current_ground);
        painter.rect_stroke(
            rect,
            CornerRadius::same(3),
            Stroke::new(1.0, style.completion_current_edge),
            StrokeKind::Inside,
        );
    }
    painter.text(
        pos2(rect.left() + style.completion_row_pad, rect.center().y),
        Align2::LEFT_CENTER,
        &completion.label,
        style.font.clone(),
        style.ink,
    );
    if let Some(detail) = &completion.detail {
        painter.text(
            pos2(rect.right() - style.completion_row_pad, rect.center().y),
            Align2::RIGHT_CENTER,
            detail,
            style.line_number_font.clone(),
            style.completion_detail_ink,
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offering(labels: &[&str]) -> Vec<Completion> {
        labels
            .iter()
            .map(|label| Completion {
                label: (*label).to_string(),
                detail: None,
                insert: (*label).to_string(),
            })
            .collect()
    }

    #[test]
    fn a_different_offer_puts_the_highlight_back_on_the_first_row() {
        let mut listing = Listing::default();
        let first = offering(&["value", "values"]);
        listing.offered(&first);
        listing.step(Step::Down, first.len());
        assert_eq!(listing.selected, 1);
        listing.offered(&offering(&["other"]));
        assert_eq!(listing.selected, 0);
    }

    #[test]
    fn the_highlight_stops_at_either_end_rather_than_wrapping_round() {
        let mut listing = Listing::default();
        let offer = offering(&["one", "two"]);
        listing.offered(&offer);
        listing.step(Step::Up, offer.len());
        assert_eq!(listing.selected, 0);
        listing.step(Step::Down, offer.len());
        listing.step(Step::Down, offer.len());
        assert_eq!(listing.selected, 1);
    }
}
