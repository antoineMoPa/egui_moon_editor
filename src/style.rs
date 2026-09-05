use egui::{Color32, FontId, Margin, Visuals};

use crate::syntax::TokenStyle;

/// How one kind of token is drawn: the three things a [`egui::TextFormat`] takes.
///
/// A look rather than a colour, because a code face usually comes as a family — telling a
/// keyword from a comment reads better as weight and slant than as one more hue — and an
/// application that has a bold and an italic file can say so here.
#[derive(Clone, Debug)]
pub struct TokenLook {
    /// The ink the run is set in.
    pub ink: Color32,
    /// The face the run is set in. Monospace, and the same advance as the rest of the text:
    /// code set in faces that measure differently no longer lines up in columns.
    pub font: FontId,
    /// Whether the run is sheared. This is egui's synthetic italic, so it works on any face —
    /// which is what a bold-italic doc comment is made of when there is no fourth font file.
    pub italics: bool,
}

/// A look for every [`TokenStyle`]: the palette a page of code is drawn from.
///
/// Built through [`from_fn`](Self::from_fn) rather than out of named fields, so an
/// application writing one down states its rule once — a colour table and a face table — in
/// place of eleven near-identical lines.
#[derive(Clone, Debug)]
pub struct SyntaxTheme {
    looks: [TokenLook; STYLES],
}

/// How many [`TokenStyle`]s there are, which is how many looks a theme holds.
const STYLES: usize = 11;

/// Where in a theme a style's look is kept.
///
/// Written as a match rather than as a table so that a [`TokenStyle`] nobody has given a look
/// to is a compile error here, rather than something that reads as plain text at runtime.
const fn slot(style: TokenStyle) -> usize {
    match style {
        TokenStyle::Keyword => 0,
        TokenStyle::Type => 1,
        TokenStyle::Function => 2,
        TokenStyle::Constant => 3,
        TokenStyle::Number => 4,
        TokenStyle::StringLit => 5,
        TokenStyle::Comment => 6,
        TokenStyle::DocComment => 7,
        TokenStyle::Attribute => 8,
        TokenStyle::Punctuation => 9,
        TokenStyle::Plain => 10,
    }
}

/// Every [`TokenStyle`], in the order [`slot`] files them in. The two are checked against
/// each other by a test rather than by the compiler, which is all that is left once the
/// looks live in an array.
const EVERY_STYLE: [TokenStyle; STYLES] = [
    TokenStyle::Keyword,
    TokenStyle::Type,
    TokenStyle::Function,
    TokenStyle::Constant,
    TokenStyle::Number,
    TokenStyle::StringLit,
    TokenStyle::Comment,
    TokenStyle::DocComment,
    TokenStyle::Attribute,
    TokenStyle::Punctuation,
    TokenStyle::Plain,
];

impl SyntaxTheme {
    /// The theme that gives each style whatever `look` says it should have.
    pub fn from_fn(mut look: impl FnMut(TokenStyle) -> TokenLook) -> Self {
        Self {
            looks: EVERY_STYLE.map(&mut look),
        }
    }

    /// How a run of `style` is drawn.
    pub fn look(&self, style: TokenStyle) -> &TokenLook {
        &self.looks[slot(style)]
    }
}

/// How an [`Editor`](crate::Editor) is drawn.
///
/// Fonts are [`FontId`]s rather than text styles, so an application with a code face of its
/// own can hand that face over instead of having the widget resolve one out of the egui
/// style. [`from_visuals`](Self::from_visuals) builds one for an application without a
/// palette to draw from.
#[derive(Clone, Debug)]
pub struct EditorStyle {
    /// The font the text is set in. It has to be monospace: the fringe is sized for digits
    /// of it, and code read in a proportional face is not code.
    pub font: FontId,
    /// The ink the text is set in.
    pub ink: Color32,
    /// The font the line numbers in the fringe are set in. Usually a size under
    /// [`font`](Self::font), so the numbers sit beside the code rather than compete with it.
    pub line_number_font: FontId,
    /// The ink the line numbers are drawn in.
    pub fringe_ink: Color32,
    /// How wide the fringe of line numbers is. The default is wide enough for five digits.
    pub fringe_width: f32,
    /// Between the edge of the text area and the text in it. This is what a [`egui::TextEdit`]
    /// keeps clear by default, and it is set here because the frame around the text is the
    /// widget's own.
    pub text_margin: Margin,
    /// What a mark is tinted with. Strong enough to pick a match out of the code, light
    /// enough to leave it readable.
    pub mark_ink: Color32,
    /// What the current mark is underlined with. The current one is underlined rather than
    /// tinted harder: a background solid enough to stand out from the other marks would take
    /// the text with it.
    pub current_mark_ink: Color32,
    /// The panel the list of things to finish a word with is drawn on.
    pub completion_ground: Color32,
    /// The line around that panel, which is what lifts it off the code behind it.
    pub completion_edge: Color32,
    /// What the current row of the list is filled with.
    pub completion_current_ground: Color32,
    /// The line around the current row. Fill and line together, the way the rest of this
    /// product marks the row a list's keyboard is on.
    pub completion_current_edge: Color32,
    /// The ink the quieter half of a row — a type, a signature, where it came from — is set
    /// in, against the right edge of the row.
    pub completion_detail_ink: Color32,
    /// How wide the list is. Fixed rather than measured off the rows in it, so a long detail
    /// on one row does not make the whole list jump wider as it is typed into.
    pub completion_width: f32,
    /// How many rows of the list are on screen at once. Past that the list scrolls under the
    /// highlight rather than growing down the page.
    pub completion_rows: usize,
    /// Between a row's text and its edges, and what a row is taller than the text in it.
    pub completion_row_pad: f32,
    /// Between the caret and the list, so the list does not sit on the line being typed.
    pub completion_gap: f32,
    /// Between the edge of the list and the rows in it.
    pub completion_margin: Margin,
    /// How each kind of token is drawn. [`ink`](Self::ink) and [`font`](Self::font) still say
    /// what the text area is worth on its own — the caret, the selection, the size the rows
    /// are measured at — and this says what each run inside it looks like.
    pub syntax: SyntaxTheme,
}

impl Default for EditorStyle {
    fn default() -> Self {
        Self::from_visuals(&Visuals::dark())
    }
}

/// The size the text is set at when the style comes from egui's own visuals.
const DEFAULT_CODE_SIZE: f32 = 12.0;

/// How wide the list of things to finish a word with is, in points: enough for an identifier
/// of a length people actually write and a short note about it beside.
const DEFAULT_COMPLETION_WIDTH: f32 = 280.0;

/// How many rows of that list are on screen at once. Enough to see that there is a choice,
/// few enough that the list does not become the page.
const DEFAULT_COMPLETION_ROWS: usize = 8;

/// The ink each kind of token gets on a light ground, and on a dark one.
///
/// A table rather than a chain of branches because it is data about a theme: restrained hues
/// of about the same weight as the text around them, so a page of code reads as prose with
/// things picked out of it rather than as a paint chart. Punctuation and plain text are not
/// in here — they come from the visuals themselves, so that the bulk of the page is whatever
/// ink the application writes everything else in.
const DEFAULT_INKS: &[(TokenStyle, u32, u32)] = &[
    (TokenStyle::Keyword, 0xa03a1f, 0xf2937a),
    (TokenStyle::Type, 0x1f5f54, 0x7ed0c2),
    (TokenStyle::Function, 0x2f5aa8, 0x8ab4f8),
    (TokenStyle::Constant, 0x5a45a8, 0xc4a2f5),
    (TokenStyle::Number, 0x8c3a86, 0xe8a5d8),
    (TokenStyle::StringLit, 0x3f6b1f, 0xa9d67f),
    (TokenStyle::Comment, 0x857a6a, 0x7f8b99),
    (TokenStyle::DocComment, 0x6f6353, 0x9aa8b6),
    (TokenStyle::Attribute, 0x8a6a12, 0xe7bd58),
];

/// A colour written the way the table above writes it.
const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

impl EditorStyle {
    /// The style that suits an egui theme: the text in the ink the rest of the UI is written
    /// in, the fringe in the weak ink beside it, and the marks in the theme's selection
    /// color.
    ///
    /// The highlighting it comes with is colour only, on the one monospace face egui has of
    /// its own — bar the comments, which are sheared, since that costs no font file. An
    /// application with a code family of its own says so in
    /// [`syntax`](EditorStyle::syntax) instead.
    pub fn from_visuals(visuals: &Visuals) -> Self {
        let accent = visuals.selection.stroke.color;
        let ink = visuals
            .override_text_color
            .unwrap_or_else(|| visuals.widgets.inactive.text_color());
        let muted = visuals.weak_text_color();
        Self {
            font: FontId::monospace(DEFAULT_CODE_SIZE),
            ink,
            line_number_font: FontId::monospace(DEFAULT_CODE_SIZE - 1.0),
            fringe_ink: muted,
            fringe_width: 46.0,
            text_margin: Margin::symmetric(4, 2),
            mark_ink: accent.linear_multiply(0.35),
            current_mark_ink: accent,
            completion_ground: visuals.window_fill,
            completion_edge: visuals.widgets.noninteractive.bg_stroke.color,
            completion_current_ground: accent.linear_multiply(0.35),
            completion_current_edge: accent,
            completion_detail_ink: muted,
            completion_width: DEFAULT_COMPLETION_WIDTH,
            completion_rows: DEFAULT_COMPLETION_ROWS,
            completion_row_pad: 6.0,
            completion_gap: 3.0,
            completion_margin: Margin::same(3),
            syntax: SyntaxTheme::from_fn(|style| TokenLook {
                ink: default_ink(style, visuals.dark_mode, ink, muted),
                font: FontId::monospace(DEFAULT_CODE_SIZE),
                italics: matches!(style, TokenStyle::Comment | TokenStyle::DocComment),
            }),
        }
    }
}

/// The ink [`DEFAULT_INKS`] asks for, falling back to the inks the visuals already carry for
/// the two styles most of a page is made of.
fn default_ink(style: TokenStyle, dark: bool, ink: Color32, muted: Color32) -> Color32 {
    let row = DEFAULT_INKS.iter().find(|(named, _, _)| *named == style);
    match row {
        Some(&(_, light, night)) => rgb(if dark { night } else { light }),
        None if style == TokenStyle::Punctuation => muted,
        None => ink,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_token_style_is_filed_where_the_theme_looks_for_it() {
        for (index, style) in EVERY_STYLE.into_iter().enumerate() {
            assert_eq!(slot(style), index, "{style:?} is filed in the wrong slot");
        }
    }

    #[test]
    fn a_theme_hands_back_the_look_it_was_given_for_a_style() {
        let theme = SyntaxTheme::from_fn(|style| TokenLook {
            ink: Color32::from_gray(slot(style) as u8),
            font: FontId::monospace(12.0),
            italics: style == TokenStyle::Comment,
        });
        assert_eq!(theme.look(TokenStyle::Number).ink, Color32::from_gray(4));
        assert!(theme.look(TokenStyle::Comment).italics);
        assert!(!theme.look(TokenStyle::Plain).italics);
    }

    #[test]
    fn the_default_highlighting_tells_a_keyword_from_a_comment_on_either_ground() {
        for visuals in [Visuals::light(), Visuals::dark()] {
            let style = EditorStyle::from_visuals(&visuals);
            let keyword = style.syntax.look(TokenStyle::Keyword);
            let comment = style.syntax.look(TokenStyle::Comment);
            assert_ne!(keyword.ink, comment.ink);
            assert_ne!(keyword.ink, style.syntax.look(TokenStyle::Plain).ink);
            assert!(comment.italics);
            assert!(!keyword.italics);
        }
    }
}
