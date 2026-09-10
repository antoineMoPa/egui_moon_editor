//! Which lines of the text are new: the ones the text it is compared against does not have.
//! For an editor on a file of a repo that is the file as it was last committed, and the new
//! lines are what the fringe draws a bar beside.

use std::{ops::Range, time::Duration};

use similar::{Algorithm, DiffOp, DiffTag, TextDiff};

/// How long a comparison may take before it settles for the answer it has. A file changed past
/// untangling in that time gets a coarser answer - more of it marked new than strictly is -
/// rather than a frame that stalls.
const COMPARISON_TIMEOUT: Duration = Duration::from_millis(20);

/// How long the text has to be left alone, in seconds, before the fringe is brought up to date
/// with it. Compared on every keystroke, a bar that comes and goes with each line typed pulls
/// the eye away from the typing.
const QUIET_BEFORE_COMPARING: f64 = 3.0;

/// How long, in seconds, a bar that was not there before takes to fade in.
const FADE_IN: f64 = 1.0;

/// The lines the fringe marks as new, and what it takes to keep them up to date without the
/// bar flickering along with the typing.
///
/// An edit does not compare the text again straight away. The bars already there move with the
/// lines they are beside, so a line typed above one does not leave it beside the wrong line,
/// and the comparison waits until the text has been left alone for [`QUIET_BEFORE_COMPARING`].
/// The bars it turns up that were not there before fade in over [`FADE_IN`]. Times are egui's
/// own clock, in seconds, which is what the caller has in hand every frame.
#[derive(Default)]
pub(crate) struct NewLines {
    /// What the text is compared against - for a file of a repo, the file as it was last
    /// committed. `None` compares against nothing and marks no line, which is an editor on text
    /// with no earlier version to be new against.
    base: Option<String>,
    /// The lines the fringe draws a bar beside, as ranges of line indexes from zero, in order.
    shown: Vec<Range<usize>>,
    /// The lines of [`shown`](Self::shown) the last comparison added, which fade in from
    /// [`arrived_at`](Self::arrived_at).
    arriving: Vec<Range<usize>>,
    arrived_at: f64,
    /// Set while an edit has not been compared yet.
    owed: Option<Owed>,
    /// How many line breaks the text had when it was last seen, which is what says how many
    /// lines an edit put in or took out.
    line_breaks: usize,
}

/// An edit the fringe has not caught up with yet.
struct Owed {
    /// When the text was last edited.
    since: f64,
    /// Where the caret was after that edit - see [`new_lines`] for what it is used for.
    caret_line: Option<usize>,
}

impl NewLines {
    /// Compare against `base` from here on, at once and with nothing fading in: the text is
    /// being opened rather than typed into.
    pub(crate) fn set_base(&mut self, base: Option<String>, text: &str) {
        self.base = base;
        self.text_replaced(text);
    }

    /// The whole text replaced, the way loading a file does: compared at once, with nothing
    /// fading in, since there was no typing to wait for the end of.
    pub(crate) fn text_replaced(&mut self, text: &str) {
        self.shown = new_lines(self.base.as_deref(), text, None);
        self.arriving = Vec::new();
        self.owed = None;
        self.line_breaks = line_breaks(text);
    }

    /// The text was edited at line `at`, counted from zero - the first line the edit touched.
    /// The bars below it move with their lines, and the comparison waits for the typing to
    /// stop.
    pub(crate) fn edited(&mut self, text: &str, at: usize, caret_line: Option<usize>, now: f64) {
        let breaks = line_breaks(text);
        let delta = breaks as isize - self.line_breaks as isize;
        self.line_breaks = breaks;
        self.shown = shifted(&self.shown, at, delta);
        self.arriving = shifted(&self.arriving, at, delta);
        self.owed = Some(Owed {
            since: now,
            caret_line,
        });
    }

    /// Compare the text again if it has been left alone long enough, and say how soon the
    /// editor has to be drawn again for the fringe to carry on by itself - at once while a bar
    /// is fading in, when the wait runs out while an edit is owed, and not at all otherwise.
    pub(crate) fn settle(&mut self, text: &str, now: f64) -> Option<Duration> {
        if let Some(owed) = self.owed.take_if(|owed| now - owed.since >= QUIET_BEFORE_COMPARING) {
            let compared = new_lines(self.base.as_deref(), text, owed.caret_line);
            self.arriving = lines_not_in(&compared, &self.shown);
            self.arrived_at = now;
            self.shown = compared;
        }
        if !self.arriving.is_empty() && now - self.arrived_at < FADE_IN {
            return Some(Duration::ZERO);
        }
        self.owed
            .as_ref()
            .map(|owed| Duration::from_secs_f64(QUIET_BEFORE_COMPARING - (now - owed.since)))
    }

    /// How strongly the bar beside the line at `index` is drawn, from nothing to full - or
    /// `None` when there is no bar beside it.
    pub(crate) fn opacity(&self, index: usize, now: f64) -> Option<f32> {
        if !is_new(&self.shown, index) {
            return None;
        }
        if !is_new(&self.arriving, index) {
            return Some(1.0);
        }
        Some(((now - self.arrived_at) / FADE_IN).clamp(0.0, 1.0) as f32)
    }

    /// The lines the fringe marks, as ranges of line indexes from zero, in order.
    pub(crate) fn lines(&self) -> &[Range<usize>] {
        &self.shown
    }
}

/// How many line breaks are in `text`, which is one less than the lines the fringe numbers.
fn line_breaks(text: &str) -> usize {
    text.bytes().filter(|&byte| byte == b'\n').count()
}

/// `ranges` moved with the lines they are beside, after an edit at line `at` put in `delta`
/// line breaks - or took out as many, when it is negative. The lines up to `at` stay where they
/// are, the lines below move by `delta`, and the lines a deletion took away take their part of a
/// range with them.
///
/// Only ever a stand-in until the next comparison: an edit that pushes the line it starts on
/// down, Enter at the start of a line, leaves that line's bar a line above it until then.
fn shifted(ranges: &[Range<usize>], at: usize, delta: isize) -> Vec<Range<usize>> {
    let removed = delta.min(0).unsigned_abs();
    let moved = |line: usize| {
        line.checked_add_signed(delta)
            .expect("a line below the edit stays below it")
    };
    ranges
        .iter()
        .flat_map(|range| {
            let above = range.start..range.end.min(at + 1);
            let below = range.start.max(at + 1 + removed)..range.end;
            let below = match below.is_empty() {
                true => below,
                false => moved(below.start)..moved(below.end),
            };
            [above, below]
        })
        .filter(|range| !range.is_empty())
        .collect()
}

/// The lines of `ranges` that `other` does not hold, as ranges in order.
fn lines_not_in(ranges: &[Range<usize>], other: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut lines: Vec<Range<usize>> = Vec::new();
    for index in ranges
        .iter()
        .flat_map(|range| range.clone())
        .filter(|&index| !is_new(other, index))
    {
        match lines.last_mut() {
            Some(last) if last.end == index => last.end += 1,
            _ => lines.push(index..index + 1),
        }
    }
    lines
}

/// The lines of `text` that `base` does not have, as ranges of line indexes counted from zero,
/// in order. A changed line is one of them: compared by line, it is the old line gone and a new
/// one in its place. With no `base` there is nothing to be new against, and no line is.
///
/// `caret_line` is the line being typed on, counted from zero, when the text has just been
/// typed into. A run of lines put in can often be read as put in at more than one place - a
/// blank line typed under a blank line is either of the two - and the comparison alone picks
/// one without knowing which was typed. The caret does know, so a run that could sit in more
/// than one place is put at the one nearest it.
fn new_lines(
    base: Option<&str>,
    text: &str,
    caret_line: Option<usize>,
) -> Vec<Range<usize>> {
    let Some(base) = base else {
        return Vec::new();
    };
    // Histogram, the algorithm a diff in git is asked for, so where a run of lines could be
    // lined up more than one way the fringe picks the same lines as new that the diff does -
    // until the caret says otherwise.
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Histogram)
        .timeout(COMPARISON_TIMEOUT)
        .diff_lines(base, text);
    let ops = diff.ops();
    // Cut the way the comparison cuts, newline included, so two lines are the same here
    // exactly when they were the same to it.
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    ops.iter()
        .enumerate()
        .filter(|(_, op)| matches!(op.tag(), DiffTag::Insert | DiffTag::Replace))
        .map(|(index, op)| match (op.tag(), caret_line) {
            (DiffTag::Insert, Some(caret)) => nearest_the_caret(ops, index, &lines, caret),
            _ => op.new_range(),
        })
        .collect()
}

/// Where the run of lines `ops[index]` put in reads best: of every place it could equally have
/// been put in, the one nearest `caret`, and where it was found when two are as near.
///
/// A run can move up a line when the line above it is the same as its own last line, and down
/// a line when the line below it is the same as its own first line - either way the text reads
/// the same and only which of the lines is called new changes. It only ever moves through the
/// lines left as they were on either side of it, so it cannot pass another change.
fn nearest_the_caret(ops: &[DiffOp], index: usize, lines: &[&str], caret: usize) -> Range<usize> {
    let found = ops[index].new_range();
    let length = found.len();
    // How far the lines left as they were reach on either side: an op beside the run that is
    // not one of those is a change, and the run stops against it.
    let floor = index
        .checked_sub(1)
        .and_then(|before| ops.get(before))
        .filter(|op| op.tag() == DiffTag::Equal)
        .map_or(found.start, |op| op.new_range().start);
    let ceiling = ops
        .get(index + 1)
        .filter(|op| op.tag() == DiffTag::Equal)
        .map_or(found.end, |op| op.new_range().end);

    let mut highest = found.start;
    while highest > floor && lines[highest - 1] == lines[highest - 1 + length] {
        highest -= 1;
    }
    let mut lowest = found.start;
    while lowest + length < ceiling && lines[lowest] == lines[lowest + length] {
        lowest += 1;
    }

    let away_from_the_caret = |start: usize| match caret {
        caret if caret < start => start - caret,
        caret if caret >= start + length => caret + 1 - (start + length),
        _ => 0,
    };
    let start = (highest..=lowest)
        .min_by_key(|&start| (away_from_the_caret(start), start.abs_diff(found.start)))
        .expect("a run can always stay where it was found");
    start..start + length
}

/// Whether the line at `index`, counted from zero, is in one of `ranges` - which are in order,
/// the way [`new_lines`] hands them back.
pub(crate) fn is_new(ranges: &[Range<usize>], index: usize) -> bool {
    let at = ranges.partition_point(|range| range.end <= index);
    ranges.get(at).is_some_and(|range| range.start <= index)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = "fn one() {}\nfn two() {}\nfn three() {}\n";

    #[test]
    fn the_text_as_committed_has_no_new_lines() {
        assert!(new_lines(Some(COMMITTED), COMMITTED, None).is_empty());
    }

    #[test]
    fn a_line_put_in_is_new_and_the_lines_around_it_are_not() {
        let text = "fn one() {}\nfn one_and_a_half() {}\nfn two() {}\nfn three() {}\n";
        assert_eq!(new_lines(Some(COMMITTED), text, None), vec![1..2]);
    }

    #[test]
    fn a_changed_line_is_new() {
        let text = "fn one() {}\nfn deux() {}\nfn three() {}\n";
        assert_eq!(new_lines(Some(COMMITTED), text, None), vec![1..2]);
    }

    /// A line taken out leaves nothing behind in the text to mark.
    #[test]
    fn a_line_taken_out_marks_nothing() {
        let text = "fn one() {}\nfn three() {}\n";
        assert!(new_lines(Some(COMMITTED), text, None).is_empty());
    }

    /// A file that was never committed is compared against nothing at all, so the whole of
    /// it is new.
    #[test]
    fn every_line_of_a_file_never_committed_is_new() {
        assert_eq!(new_lines(Some(""), COMMITTED, None), vec![0..3]);
    }

    #[test]
    fn with_nothing_to_compare_against_no_line_is_new() {
        assert!(new_lines(None, COMMITTED, None).is_empty());
    }

    /// Enter at the end of a title with a blank line under it: the blank line typed is the
    /// one the caret went down onto, not the one that was already there below it.
    #[test]
    fn a_blank_line_typed_over_a_blank_line_is_the_one_the_caret_is_on() {
        let base = "# moon-dev-tools\n\nA collection of tools.\n";
        let text = "# moon-dev-tools\n\n\nA collection of tools.\n";
        assert_eq!(new_lines(Some(base), text, Some(1)), vec![1..2]);
        assert_eq!(new_lines(Some(base), text, Some(2)), vec![2..3]);
    }

    /// Enter at the start of a line leaves the caret on the line pushed down, below the blank
    /// line put in: the nearest place the run can be is right above it.
    #[test]
    fn a_run_the_caret_is_not_on_is_put_as_near_it_as_it_can_be() {
        let base = "a\n\n\nb\n";
        let text = "a\n\n\n\nb\n";
        assert_eq!(new_lines(Some(base), text, Some(4)), vec![3..4]);
        assert_eq!(new_lines(Some(base), text, Some(0)), vec![1..2]);
    }

    /// A function added under another: read one way the run starts on the closing brace of
    /// the one above. Typed at the end, the caret says the run is the new function itself.
    #[test]
    fn a_function_added_under_another_is_the_new_function() {
        let base = "fn a() {\n}\n";
        let text = "fn a() {\n}\n\nfn b() {\n}\n";
        assert_eq!(new_lines(Some(base), text, Some(4)), vec![2..5]);
    }

    /// A run stops against a change beside it rather than slipping through it to reach the
    /// caret: three blank lines, the first changed, the second left as it was and the third put
    /// in. The run could read as any of them by content alone, but only the second is not
    /// already another change.
    #[test]
    fn a_run_does_not_move_past_another_change() {
        let lines = ["\n", "\n", "\n"];
        let ops = [
            DiffOp::Replace {
                old_index: 0,
                old_len: 1,
                new_index: 0,
                new_len: 1,
            },
            DiffOp::Equal {
                old_index: 1,
                new_index: 1,
                len: 1,
            },
            DiffOp::Insert {
                old_index: 2,
                new_index: 2,
                new_len: 1,
            },
        ];
        assert_eq!(nearest_the_caret(&ops, 2, &lines, 0), 1..2);
    }

    /// A line typed in is not marked while the typing goes on, and is once the text has been
    /// left alone for long enough.
    #[test]
    fn typing_is_compared_only_once_the_text_is_left_alone() {
        let mut new = NewLines::default();
        new.set_base(Some("a\n".to_string()), "a\n");
        let text = "a\nb\n";
        new.edited(text, 0, Some(1), 10.0);

        assert_eq!(new.settle(text, 11.0), Some(Duration::from_secs(2)));
        assert!(new.lines().is_empty(), "compared while still typing");

        // Fading in, so the editor is asked back at once.
        assert_eq!(new.settle(text, 13.0), Some(Duration::ZERO));
        assert_eq!(new.lines(), vec![1..2]);
    }

    /// Each edit starts the wait over.
    #[test]
    fn another_edit_starts_the_wait_over() {
        let mut new = NewLines::default();
        new.set_base(Some("a\n".to_string()), "a\n");
        new.edited("a\nb", 0, Some(1), 10.0);
        new.edited("a\nb\n", 1, Some(2), 12.0);

        new.settle("a\nb\n", 14.0);
        assert!(new.lines().is_empty(), "compared two seconds after the last edit");
        new.settle("a\nb\n", 15.0);
        assert_eq!(new.lines(), vec![1..2]);
    }

    /// A bar the comparison turned up fades in over a second, and once it has, the editor is
    /// not asked back any more.
    #[test]
    fn a_new_bar_fades_in_over_a_second() {
        let mut new = NewLines::default();
        new.set_base(Some("a\n".to_string()), "a\n");
        new.edited("a\nb\n", 0, Some(1), 10.0);
        new.settle("a\nb\n", 13.0);

        assert_eq!(new.opacity(1, 13.0), Some(0.0));
        assert_eq!(new.opacity(1, 13.5), Some(0.5));
        assert_eq!(new.opacity(1, 14.5), Some(1.0));
        assert_eq!(new.opacity(0, 13.5), None);
        assert_eq!(new.settle("a\nb\n", 14.5), None);
    }

    /// A bar already there when the file opened is drawn in full, and moves down with its line
    /// when a line is typed above it - with no fade, since it was never gone.
    #[test]
    fn a_bar_already_there_moves_with_its_line() {
        let mut new = NewLines::default();
        new.set_base(Some("a\n".to_string()), "a\nb\n");
        assert_eq!(new.lines(), vec![1..2]);
        assert_eq!(new.opacity(1, 0.0), Some(1.0));

        new.edited("a\n\nb\n", 0, Some(1), 10.0);
        assert_eq!(new.lines(), vec![2..3]);
        assert_eq!(new.opacity(2, 10.0), Some(1.0));
    }

    /// Lines below an edit move by as many lines as it put in or took out; the lines it took
    /// out take their part of a range with them, and the lines above it stay put.
    // A list of one range is what is meant here - one run of lines marked - not a range
    // written as a list by mistake, which is what the lint is after.
    #[allow(clippy::single_range_in_vec_init)]
    #[test]
    fn the_lines_below_an_edit_move_with_it() {
        assert_eq!(shifted(&[1..2, 4..6], 2, 1), vec![1..2, 5..7]);
        assert_eq!(shifted(&[1..2, 4..6], 2, -1), vec![1..2, 3..5]);
        assert_eq!(shifted(&[2..5], 2, -2), vec![2..3]);
        assert_eq!(shifted(&[1..3], 1, 2), vec![1..2, 4..5]);
    }

    #[test]
    fn a_line_is_new_when_a_range_holds_it() {
        let ranges = [1..3, 5..6];
        let new: Vec<usize> = (0..8).filter(|&index| is_new(&ranges, index)).collect();
        assert_eq!(new, vec![1, 2, 5]);
    }
}
