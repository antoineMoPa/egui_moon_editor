use egui::{Color32, FontId, Margin, Visuals};

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
}

impl Default for EditorStyle {
    fn default() -> Self {
        Self::from_visuals(&Visuals::dark())
    }
}

/// The size the text is set at when the style comes from egui's own visuals.
const DEFAULT_CODE_SIZE: f32 = 12.0;

impl EditorStyle {
    /// The style that suits an egui theme: the text in the ink the rest of the UI is written
    /// in, the fringe in the weak ink beside it, and the marks in the theme's selection
    /// color.
    pub fn from_visuals(visuals: &Visuals) -> Self {
        let accent = visuals.selection.stroke.color;
        Self {
            font: FontId::monospace(DEFAULT_CODE_SIZE),
            ink: visuals
                .override_text_color
                .unwrap_or_else(|| visuals.widgets.inactive.text_color()),
            line_number_font: FontId::monospace(DEFAULT_CODE_SIZE - 1.0),
            fringe_ink: visuals.weak_text_color(),
            fringe_width: 46.0,
            text_margin: Margin::symmetric(4, 2),
            mark_ink: accent.linear_multiply(0.35),
            current_mark_ink: accent,
        }
    }
}
