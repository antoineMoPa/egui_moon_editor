//! Reading code as code: which run of which line is a keyword, a string, a comment.
//!
//! What comes out is a token stream rather than a laid-out [`egui::text::LayoutJob`], because
//! two very different callers want it. The editor holds a buffer and draws the window of it
//! that is on screen; a diff view holds nothing, paints its own rows, and asks about a hunk
//! it already has in hand. A stream of ranges and looks is what both can build their own
//! layout from, and it leaves the palette to the caller's theme, where it belongs.
//!
//! Hence the two ways in. [`highlight`] is the one-shot: short text in, tokens out, nothing
//! kept. [`Highlighter`] is for a buffer being edited — it remembers where the parser had got
//! to, so scrolling to line 40,000 of a file does not re-read the 39,999 lines above it every
//! frame.
//!
//! Without the `syntax` feature the shape of all of this is the same and every token comes
//! back [`TokenStyle::Plain`]: a caller written against it keeps compiling and keeps drawing,
//! it just draws code in one colour.

use std::ops::Range;

#[cfg(feature = "syntax")]
mod scopes;

/// One run of a line that carries a look of its own.
///
/// The tokens of a line cover it end to end and in order, so a caller can walk them straight
/// into a layout without minding the gaps between them — the parts of the line nothing has an
/// opinion about come back as [`TokenStyle::Plain`] runs rather than as holes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Token {
    /// Byte range **relative to the start of its own line**, not to the whole text. A line's
    /// tokens are therefore worth the same to a caller who holds only that line — a diff row
    /// — as to one who holds the whole file.
    ///
    /// The line's trailing newline is not part of any token: what is offered is the text a
    /// caller would paint.
    pub range: Range<usize>,
    /// What the run is, as far as a theme is concerned.
    pub style: TokenStyle,
}

/// The kinds of thing a theme gives a look to.
///
/// Deliberately far short of the scopes syntect knows: code reads better in eight colours
/// than in forty, and a caller has to be able to write down a palette for all of them without
/// it becoming a project of its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TokenStyle {
    /// `if`, `fn`, `pub`, `async` — the words the language reserved for itself.
    Keyword,
    /// The name of a type, class, trait or tag.
    Type,
    /// The name of something callable, at its definition or at a call.
    Function,
    /// A named constant, a language literal like `true`, an escape inside a string.
    Constant,
    /// A numeric literal, split out from the other constants because a page of code is often
    /// read for its numbers.
    Number,
    /// A string literal. Named for the length of it because `String` is taken.
    StringLit,
    /// An ordinary comment: an aside a reader usually skips.
    Comment,
    /// A documentation comment: prose a reader usually wants, so worth a look of its own.
    DocComment,
    /// An attribute, annotation or decorator — metadata bolted to a declaration.
    Attribute,
    /// Brackets, commas, operators: the shape of the line rather than its vocabulary.
    Punctuation,
    /// Everything else, including every token when the `syntax` feature is off.
    #[default]
    Plain,
}

/// Above this much text, [`highlight`] gives up and calls the lot plain.
///
/// The one-shot entry point has no idea what it is being handed — a diff hunk, or a
/// pathological one the size of a dictionary — and parsing megabytes inside a frame would
/// stall the UI. A caller with a buffer that big wants [`Highlighter`], which never does more
/// work than the window it was asked about.
const ONE_SHOT_CEILING: usize = 2 * 1024 * 1024;

/// How many lines apart the parser's resumable positions are kept.
///
/// The trade is memory against how far back a jump has to re-read: a checkpoint is a cloned
/// parser state, so one per line would be extravagant, and 256 lines is a few milliseconds of
/// re-reading at worst.
const CHECKPOINT_STRIDE: usize = 256;

/// The lines of `text`, each still carrying its newline.
///
/// Split inclusively because the bundled grammars are the newline ones: a grammar's rules can
/// key off the end of the line, and handing over a line with the newline shaved off quietly
/// changes what it matches. It also means a trailing newline does not add an empty last line
/// — `"a\n"` is one line, the same count an editor's fringe shows.
fn lines_of(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// The tokens of `text`, one [`Vec`] per line, in line order.
///
/// For short texts — a diff hunk's size. Above a couple of megabytes every line comes back
/// plain rather than stalling the frame that asked. A caller editing a buffer wants
/// [`Highlighter`] instead.
///
/// ```
/// # use egui_moon_editor::{highlight, Language};
/// let lines = highlight(&Language::of_path("main.rs"), "fn main() {}\nlet x = 1;\n");
/// assert_eq!(lines.len(), 2);
/// ```
pub fn highlight(language: &Language, text: &str) -> Vec<Vec<Token>> {
    let lines = lines_of(text);
    if text.len() > ONE_SHOT_CEILING {
        return lines.iter().map(|line| plain_tokens(line)).collect();
    }
    let mut cursor = Cursor::start(language);
    lines.iter().map(|line| cursor.read(line)).collect()
}

/// A buffer's worth of highlighting, kept between frames: the parser checkpoints, and the
/// tokens worked out so far.
///
/// A grammar can only be read forwards — whether line 40,000 is inside a block comment is
/// only knowable from the 39,999 lines above it — so a widget that draws one screenful at a
/// time either re-reads the file every frame or remembers where the parser had got to. This
/// remembers. Nothing is read until a window asks for it, so opening a huge file and looking
/// at the top of it costs a screenful of work, not a file's worth.
///
/// The tokens are kept for the text they were read from. An edit makes everything from that
/// line down a guess, so tell it: [`invalidate_from`](Self::invalidate_from).
pub struct Highlighter {
    language: Language,
    /// The tokens of each line, as far as any window has asked. `None` is "not read yet".
    tokens: Vec<Option<Vec<Token>>>,
    /// The parser as it stood before line `index * CHECKPOINT_STRIDE`, where it has ever
    /// stood there.
    checkpoints: Vec<Option<Cursor>>,
}

impl Highlighter {
    /// A highlighter for a buffer written in `language`, holding nothing yet.
    pub fn new(language: Language) -> Self {
        Self {
            language,
            tokens: Vec::new(),
            checkpoints: Vec::new(),
        }
    }

    /// Work out the tokens for `lines` of `text`, resuming from the nearest checkpoint at or
    /// before the range.
    ///
    /// Cheap to call every frame with the same range: a window already read is noticed and
    /// nothing is parsed. `text` has to be the text the earlier calls were about — after an
    /// edit, [`invalidate_from`](Self::invalidate_from) first.
    pub fn prepare(&mut self, text: &str, lines: Range<usize>) {
        let all = lines_of(text);
        let end = lines.end.min(all.len());
        if lines.start >= end {
            return;
        }
        if self.tokens.len() < all.len() {
            self.tokens.resize_with(all.len(), || None);
        }
        if self.tokens[lines.start..end].iter().all(Option::is_some) {
            return;
        }

        let (mut at, mut cursor) = self.resume_at(lines.start);
        while at < end {
            if at.is_multiple_of(CHECKPOINT_STRIDE) {
                self.keep_checkpoint(at, &cursor);
            }
            self.tokens[at] = Some(cursor.read(all[at]));
            at += 1;
        }
    }

    /// The tokens of one line, counting from zero.
    ///
    /// Empty when [`prepare`](Self::prepare) has not reached it — which is also what an empty
    /// line gives back, since a caller paints the same nothing either way.
    pub fn tokens_on(&self, line: usize) -> &[Token] {
        match self.tokens.get(line) {
            Some(Some(tokens)) => tokens,
            _ => &[],
        }
    }

    /// Forget everything from `line` on — what an edit there invalidates.
    ///
    /// What is above is untouched, because the parser reaching that line never looked below
    /// it. This is the whole cost of an edit: the lines under the caret are read again the
    /// next time they are drawn.
    pub fn invalidate_from(&mut self, line: usize) {
        self.tokens.truncate(line);
        // A checkpoint is the parser as it stood *before* its line, so the one exactly at
        // `line` survives: nothing that changed can have gone into it.
        let kept = line / CHECKPOINT_STRIDE + 1;
        self.checkpoints.truncate(kept);
    }

    /// The furthest checkpoint at or before `line`, and the line it stands before. Falls back
    /// to the top of the file, which is a checkpoint that never has to be stored.
    fn resume_at(&self, line: usize) -> (usize, Cursor) {
        let nearest = (line / CHECKPOINT_STRIDE).min(self.checkpoints.len().saturating_sub(1));
        for index in (0..=nearest).rev() {
            if let Some(Some(cursor)) = self.checkpoints.get(index) {
                return (index * CHECKPOINT_STRIDE, cursor.clone());
            }
        }
        (0, Cursor::start(&self.language))
    }

    /// Remember the parser as it stands before `line`, which the caller has checked is a line
    /// a checkpoint belongs at.
    fn keep_checkpoint(&mut self, line: usize, cursor: &Cursor) {
        let index = line / CHECKPOINT_STRIDE;
        if self.checkpoints.len() <= index {
            self.checkpoints.resize_with(index + 1, || None);
        }
        self.checkpoints[index] = Some(cursor.clone());
    }
}

/// One line's worth of plain tokens: the whole line, minus its newline, in one run.
fn plain_tokens(line: &str) -> Vec<Token> {
    let len = painted_len(line);
    if len == 0 {
        return Vec::new();
    }
    vec![Token {
        range: 0..len,
        style: TokenStyle::Plain,
    }]
}

/// How much of a line a caller would paint: all of it but the newline the grammar needed.
fn painted_len(line: &str) -> usize {
    line.trim_end_matches('\n').trim_end_matches('\r').len()
}

#[cfg(feature = "syntax")]
mod backend {
    use std::sync::OnceLock;

    use syntect::parsing::{ParseState, ScopeStack, ScopeStackOp, SyntaxReference, SyntaxSet};

    use super::{Token, TokenStyle, painted_len, plain_tokens, scopes::style_of_scope};

    /// The bundled grammars, built once.
    ///
    /// Building the set is some tens of milliseconds and the result is immutable, so it is
    /// paid for by whoever opens the first file and never again.
    fn grammars() -> &'static SyntaxSet {
        static GRAMMARS: OnceLock<SyntaxSet> = OnceLock::new();
        // The newline variant: the grammars expect each line to still end in one.
        GRAMMARS.get_or_init(SyntaxSet::load_defaults_newlines)
    }

    /// What a file is written in.
    ///
    /// Built from a path, so a language that means nothing cannot be asked for, and cheap to
    /// clone — it is a borrow of a grammar the process already holds.
    #[derive(Clone)]
    pub struct Language(Option<&'static SyntaxReference>);

    impl Language {
        /// The language of a file at `path`, by its extension.
        ///
        /// Falls back to plain and never fails: an editor asked to open something odd should
        /// show it, not refuse it.
        pub fn of_path(path: &str) -> Self {
            let Some(extension) = extension_of(path) else {
                return Self::plain();
            };
            Self(grammars().find_syntax_by_extension(extension))
        }

        /// No language: everything comes back [`TokenStyle::Plain`].
        pub fn plain() -> Self {
            Self(None)
        }
    }

    /// The extension of the file `path` names, if it has one. Only the last component is
    /// looked at, so a dot in a directory name up the path is not mistaken for one.
    fn extension_of(path: &str) -> Option<&str> {
        let name = path.rsplit(['/', '\\']).next()?;
        let (stem, extension) = name.rsplit_once('.')?;
        // A dotfile is all stem: `.gitignore` is not a file with a `gitignore` extension.
        (!stem.is_empty()).then_some(extension)
    }

    /// The parser part way through a text: how far the grammar's rules have got, and which
    /// scopes are open around the next line.
    ///
    /// Both halves clone, which is what makes a checkpoint possible at all.
    #[derive(Clone)]
    pub(super) struct Cursor(Option<Parsing>);

    /// The state a real grammar needs; absent for a plain file.
    #[derive(Clone)]
    struct Parsing {
        state: ParseState,
        stack: ScopeStack,
    }

    impl Cursor {
        /// The parser at the top of a file written in `language`.
        pub(super) fn start(language: &Language) -> Self {
            Self(language.0.map(|syntax| Parsing {
                state: ParseState::new(syntax),
                stack: ScopeStack::new(),
            }))
        }

        /// The tokens of the next line — which still carries its newline — moving the parser
        /// past it.
        pub(super) fn read(&mut self, line: &str) -> Vec<Token> {
            let Some(parsing) = self.0.as_mut() else {
                return plain_tokens(line);
            };
            // A grammar can refuse a line: a regex that runs away, or a stack the rules
            // pushed further than syntect allows. That is the file's doing rather than a
            // broken contract here, so the line is shown plainly and reading carries on.
            let Ok(ops) = parsing.state.parse_line(line, grammars()) else {
                return plain_tokens(line);
            };
            runs_of(line, &ops, &mut parsing.stack)
        }
    }

    /// The line cut into runs at the points the scope stack changed under it.
    fn runs_of(line: &str, ops: &[(usize, ScopeStackOp)], stack: &mut ScopeStack) -> Vec<Token> {
        let end = painted_len(line);
        let mut tokens: Vec<Token> = Vec::new();
        let mut at = 0;
        for (offset, op) in ops {
            push_run(&mut tokens, at..(*offset).min(end), style_of(stack));
            at = (*offset).min(end);
            if stack.apply(op).is_err() {
                // Same as a refused line: the rest of it is read as whatever is open now.
                break;
            }
        }
        push_run(&mut tokens, at..end, style_of(stack));
        tokens
    }

    /// Add a run to the line, unless it is empty — and join it to the one before when they
    /// look the same, so a caller is not handed a token per character.
    fn push_run(tokens: &mut Vec<Token>, range: std::ops::Range<usize>, style: TokenStyle) {
        if range.is_empty() {
            return;
        }
        match tokens.last_mut() {
            Some(last) if last.style == style && last.range.end == range.start => {
                last.range.end = range.end;
            }
            _ => tokens.push(Token { range, style }),
        }
    }

    /// The look the open scopes ask for.
    ///
    /// Read from the top of the stack down, so the innermost scope wins where the table has
    /// something to say about it, and one it says nothing about — `meta.block`, say — falls
    /// through to the scope enclosing it.
    fn style_of(stack: &ScopeStack) -> TokenStyle {
        stack
            .scopes
            .iter()
            .rev()
            .find_map(|scope| style_of_scope(&scope.build_string()))
            .unwrap_or_default()
    }
}

#[cfg(not(feature = "syntax"))]
mod backend {
    use super::{Token, plain_tokens};

    /// What a file is written in.
    ///
    /// Without the `syntax` feature there is only the one language — none — but the type is
    /// still built from a path, so a caller reads and compiles the same either way.
    #[derive(Clone)]
    pub struct Language(());

    impl Language {
        /// The language of a file at `path`, by its extension. Falls back to plain; never
        /// fails, and without the `syntax` feature that fallback is all there is.
        pub fn of_path(_path: &str) -> Self {
            Self::plain()
        }

        /// No language: everything comes back [`TokenStyle::Plain`](super::TokenStyle::Plain).
        pub fn plain() -> Self {
            Self(())
        }
    }

    /// The parser part way through a text — with nothing to parse, a place to stand and
    /// nothing more.
    #[derive(Clone)]
    pub(super) struct Cursor;

    impl Cursor {
        /// The parser at the top of a file written in `language`.
        pub(super) fn start(_language: &Language) -> Self {
            Self
        }

        /// The tokens of the next line, moving the parser past it.
        pub(super) fn read(&mut self, line: &str) -> Vec<Token> {
            plain_tokens(line)
        }
    }
}

use backend::Cursor;
pub use backend::Language;

#[cfg(test)]
mod tests {
    use super::*;

    /// Long enough to have real checkpoints under it, with a block comment opened well above
    /// the middle and closed well below it: the window in the middle can only be read right
    /// by something that carried the open comment down to it.
    fn a_file_with_a_comment_across_the_middle() -> String {
        let mut text = String::new();
        for line in 0..600 {
            if line == 100 {
                text.push_str("/* the comment opens here\n");
            } else if line == 400 {
                text.push_str("still inside, and now it closes */\n");
            } else {
                text.push_str(&format!("pub const LINE_{line}: u32 = {line};\n"));
            }
        }
        text
    }

    fn tokens_read_over(text: &str, lines: Range<usize>) -> Vec<Vec<Token>> {
        let mut highlighter = Highlighter::new(Language::of_path("buffer.rs"));
        highlighter.prepare(text, lines.clone());
        lines
            .map(|line| highlighter.tokens_on(line).to_vec())
            .collect()
    }

    #[test]
    fn a_trailing_newline_does_not_add_a_line_that_is_not_there() {
        assert_eq!(highlight(&Language::plain(), "one\ntwo\n").len(), 2);
        assert_eq!(highlight(&Language::plain(), "one\ntwo").len(), 2);
        assert!(highlight(&Language::plain(), "").is_empty());
    }

    #[test]
    fn the_tokens_of_a_line_cover_it_end_to_end_without_its_newline() {
        let line = "let sum = 1 + 2; // and a comment\n";
        let tokens = &highlight(&Language::of_path("src/main.rs"), line)[0];
        assert_eq!(tokens.first().unwrap().range.start, 0);
        assert_eq!(tokens.last().unwrap().range.end, line.len() - 1);
        for pair in tokens.windows(2) {
            assert_eq!(pair[0].range.end, pair[1].range.start, "a gap between runs");
        }
    }

    #[test]
    fn a_file_with_no_extension_or_an_extension_nobody_knows_is_read_plainly() {
        for path in ["/etc/hosts", "notes.qqzz", "/home/someone.dir/README"] {
            let lines = highlight(&Language::of_path(path), "fn main() {}\n");
            assert!(
                lines[0]
                    .iter()
                    .all(|token| token.style == TokenStyle::Plain),
                "{path} was read as something"
            );
        }
    }

    #[cfg(feature = "syntax")]
    #[test]
    fn a_rust_file_is_read_as_rust_rather_than_as_one_long_plain_run() {
        let lines = highlight(&Language::of_path("src/main.rs"), "fn main() {}\n");
        let styles: Vec<TokenStyle> = lines[0].iter().map(|token| token.style).collect();
        assert!(styles.contains(&TokenStyle::Keyword), "{styles:?}");
        assert!(styles.contains(&TokenStyle::Function), "{styles:?}");
    }

    #[cfg(feature = "syntax")]
    #[test]
    fn a_doc_comment_and_the_comment_below_it_are_told_apart_in_real_code() {
        let text = "/// what it is for\n// a passing note\n";
        let lines = highlight(&Language::of_path("lib.rs"), text);
        assert_eq!(lines[0][0].style, TokenStyle::DocComment);
        assert_eq!(lines[1][0].style, TokenStyle::Comment);
    }

    #[cfg(feature = "syntax")]
    #[test]
    fn the_quotes_around_a_string_are_part_of_the_string() {
        let lines = highlight(&Language::of_path("lib.rs"), "let greeting = \"hello\";\n");
        let quoted = lines[0]
            .iter()
            .find(|token| token.style == TokenStyle::StringLit)
            .unwrap();
        assert_eq!(
            &"let greeting = \"hello\";"[quoted.range.clone()],
            "\"hello\""
        );
    }

    #[cfg(not(feature = "syntax"))]
    #[test]
    fn without_the_feature_a_line_is_one_plain_run_and_the_shape_is_unchanged() {
        let lines = highlight(
            &Language::of_path("src/main.rs"),
            "fn main() {}\nlet x = 1;\n",
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), 1);
        assert_eq!(lines[0][0].style, TokenStyle::Plain);
        assert_eq!(lines[0][0].range, 0.."fn main() {}".len());
    }

    #[test]
    fn a_window_from_the_middle_of_a_file_reads_the_same_as_reading_the_file_whole() {
        let text = a_file_with_a_comment_across_the_middle();
        let whole = highlight(&Language::of_path("buffer.rs"), &text);
        let window = 300..320;
        assert_eq!(tokens_read_over(&text, window.clone()), whole[window]);
    }

    #[cfg(feature = "syntax")]
    #[test]
    fn a_window_inside_a_comment_opened_hundreds_of_lines_above_it_is_still_a_comment() {
        let text = a_file_with_a_comment_across_the_middle();
        for line in tokens_read_over(&text, 300..320) {
            assert!(
                line.iter().all(|token| token.style == TokenStyle::Comment),
                "{line:?}"
            );
        }
    }

    #[test]
    fn preparing_the_same_window_twice_says_the_same_thing() {
        let text = a_file_with_a_comment_across_the_middle();
        let mut highlighter = Highlighter::new(Language::of_path("buffer.rs"));
        highlighter.prepare(&text, 300..320);
        let once: Vec<Token> = highlighter.tokens_on(305).to_vec();
        highlighter.prepare(&text, 300..320);
        assert_eq!(highlighter.tokens_on(305), once.as_slice());
    }

    #[test]
    fn a_line_no_window_has_reached_has_no_tokens_to_hand_over_yet() {
        let text = a_file_with_a_comment_across_the_middle();
        let mut highlighter = Highlighter::new(Language::of_path("buffer.rs"));
        highlighter.prepare(&text, 0..10);
        assert!(!highlighter.tokens_on(5).is_empty());
        assert!(highlighter.tokens_on(500).is_empty());
    }

    #[test]
    fn invalidating_a_line_forgets_what_is_below_it_and_keeps_what_is_above() {
        let text = a_file_with_a_comment_across_the_middle();
        let mut highlighter = Highlighter::new(Language::of_path("buffer.rs"));
        highlighter.prepare(&text, 0..600);
        assert!(!highlighter.tokens_on(450).is_empty());

        highlighter.invalidate_from(300);
        assert!(!highlighter.tokens_on(299).is_empty());
        assert!(highlighter.tokens_on(300).is_empty());
        assert!(highlighter.tokens_on(450).is_empty());
        // And the checkpoint below the edit went with it, so nothing stale can be resumed
        // from: only the one at line 256, which the edit is below.
        assert_eq!(highlighter.checkpoints.len(), 300 / CHECKPOINT_STRIDE + 1);
    }

    #[test]
    fn what_was_forgotten_is_read_back_the_same_as_it_was_the_first_time() {
        let text = a_file_with_a_comment_across_the_middle();
        let mut highlighter = Highlighter::new(Language::of_path("buffer.rs"));
        highlighter.prepare(&text, 0..600);
        let before: Vec<Token> = highlighter.tokens_on(450).to_vec();
        highlighter.invalidate_from(300);
        highlighter.prepare(&text, 440..460);
        assert_eq!(highlighter.tokens_on(450), before.as_slice());
    }

    #[test]
    fn opening_a_huge_file_and_looking_at_the_top_does_not_read_the_rest_of_it() {
        let text: String = (0..50_000)
            .map(|line| format!("pub const LINE_{line}: u32 = {line};\n"))
            .collect();
        let mut highlighter = Highlighter::new(Language::of_path("huge.rs"));
        highlighter.prepare(&text, 0..40);

        let read = highlighter
            .tokens
            .iter()
            .filter(|line| line.is_some())
            .count();
        assert_eq!(read, 40, "more lines were read than the window asked about");
        assert_eq!(
            highlighter.checkpoints.len(),
            1,
            "the parser was carried past the window"
        );
    }
}
