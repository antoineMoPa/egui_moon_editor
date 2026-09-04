//! What a grammar's scope names mean to us.
//!
//! syntect hands back scope names out of a `TextMate` grammar — some forty of them for Rust
//! alone, before the per-language suffixes. A theme that gave each one a look of its own
//! would be a paint chart rather than a page of code, so the table below is where that fan of
//! names is narrowed to the handful of looks a reader actually distinguishes.
//!
//! It is a table and not a chain of `if`s on purpose: the mapping is data about languages,
//! and adding a language should be adding a row, not a branch.

use super::TokenStyle;

/// Scope-name prefixes and the look each one asks for.
///
/// Order here is not what decides a match — the longest prefix wins, so
/// `comment.line.documentation` beats `comment` wherever both fit — so rows can be grouped by
/// what they are about rather than by length. A prefix matches only on a whole dotted
/// segment: `comment` covers `comment.line.rust` but not `commentary`.
const SCOPE_STYLES: &[(&str, TokenStyle)] = &[
    // Doc comments carry prose a reader wants to read, ordinary comments carry asides they
    // usually want to skip, and the two want telling apart at a glance.
    ("comment.line.documentation", TokenStyle::DocComment),
    ("comment.block.documentation", TokenStyle::DocComment),
    ("comment", TokenStyle::Comment),
    // A string is one colour all the way through, except the escapes, which are the one
    // thing in it that is not literal text.
    ("string", TokenStyle::StringLit),
    ("constant.character.escape", TokenStyle::Constant),
    ("constant.numeric", TokenStyle::Number),
    ("constant", TokenStyle::Constant),
    ("variable.other.constant", TokenStyle::Constant),
    ("support.constant", TokenStyle::Constant),
    // `let`, `fn`, `pub`, `async`: grammars file these under storage rather than keyword, but
    // to a reader they are the same kind of word.
    ("keyword", TokenStyle::Keyword),
    ("storage", TokenStyle::Keyword),
    ("variable.language", TokenStyle::Keyword),
    // Operators sit with the brackets and commas: the shape of the line, not its vocabulary.
    ("keyword.operator", TokenStyle::Punctuation),
    ("punctuation.separator", TokenStyle::Punctuation),
    ("punctuation.terminator", TokenStyle::Punctuation),
    ("punctuation.section", TokenStyle::Punctuation),
    ("punctuation.accessor", TokenStyle::Punctuation),
    // `punctuation.definition` is deliberately absent: it is the `///` of a doc comment and
    // the quotes around a string, and a reader wants those the colour of the thing they open
    // rather than the colour of a comma. Left out, they fall through to the scope enclosing
    // them, which is exactly that thing.
    // Names of things that have a shape.
    ("entity.name.type", TokenStyle::Type),
    ("entity.name.class", TokenStyle::Type),
    ("entity.name.struct", TokenStyle::Type),
    ("entity.name.enum", TokenStyle::Type),
    ("entity.name.trait", TokenStyle::Type),
    ("entity.name.interface", TokenStyle::Type),
    ("entity.name.namespace", TokenStyle::Type),
    ("entity.name.module", TokenStyle::Type),
    ("entity.name.tag", TokenStyle::Type),
    ("entity.other.inherited-class", TokenStyle::Type),
    ("support.type", TokenStyle::Type),
    ("support.class", TokenStyle::Type),
    // Names of things that can be called.
    ("entity.name.function", TokenStyle::Function),
    ("entity.name.macro", TokenStyle::Function),
    ("support.function", TokenStyle::Function),
    ("variable.function", TokenStyle::Function),
    // Attributes, annotations, decorators: the same idea under three names, and all of them
    // metadata bolted to the declaration below rather than part of it.
    ("entity.other.attribute-name", TokenStyle::Attribute),
    ("meta.attribute", TokenStyle::Attribute),
    ("meta.annotation", TokenStyle::Attribute),
    ("meta.decorator", TokenStyle::Attribute),
];

/// The look `scope` asks for, or nothing when the table has no row that fits.
///
/// Nothing is not the same as [`TokenStyle::Plain`]: a caller walking a scope stack wants to
/// carry on down to the enclosing scope when the innermost one means nothing to us, so that
/// the punctuation inside an attribute is still attribute-coloured.
pub(super) fn style_of_scope(scope: &str) -> Option<TokenStyle> {
    let mut best: Option<(usize, TokenStyle)> = None;
    for &(prefix, style) in SCOPE_STYLES {
        if !covers(prefix, scope) {
            continue;
        }
        if best.is_none_or(|(len, _)| prefix.len() > len) {
            best = Some((prefix.len(), style));
        }
    }
    best.map(|(_, style)| style)
}

/// Whether `prefix` names `scope` or an ancestor of it, counting in dotted segments so that
/// `comment` does not claim `commentary`.
fn covers(prefix: &str, scope: &str) -> bool {
    scope.starts_with(prefix)
        && (scope.len() == prefix.len() || scope.as_bytes()[prefix.len()] == b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_doc_comment_wins_over_the_plain_comment_it_is_also_a_kind_of() {
        assert_eq!(
            style_of_scope("comment.line.documentation.rust"),
            Some(TokenStyle::DocComment)
        );
        assert_eq!(
            style_of_scope("comment.line.double-slash.rust"),
            Some(TokenStyle::Comment)
        );
    }

    #[test]
    fn an_operator_is_punctuation_rather_than_the_keyword_it_is_filed_under() {
        assert_eq!(
            style_of_scope("keyword.operator.arithmetic.rust"),
            Some(TokenStyle::Punctuation)
        );
        assert_eq!(
            style_of_scope("keyword.control.rust"),
            Some(TokenStyle::Keyword)
        );
    }

    #[test]
    fn a_scope_the_table_says_nothing_about_asks_for_no_look_at_all() {
        assert_eq!(style_of_scope("meta.block.rust"), None);
        assert_eq!(style_of_scope("source.rust"), None);
    }

    #[test]
    fn the_marks_that_open_a_comment_or_a_string_are_left_to_the_thing_they_open() {
        assert_eq!(style_of_scope("punctuation.definition.comment.rust"), None);
        assert_eq!(style_of_scope("punctuation.definition.string.begin.rust"), None);
        assert_eq!(
            style_of_scope("punctuation.separator.rust"),
            Some(TokenStyle::Punctuation)
        );
    }

    #[test]
    fn a_prefix_only_matches_on_a_whole_dotted_segment() {
        assert_eq!(style_of_scope("commentary.made.up"), None);
        assert_eq!(style_of_scope("stringly.typed"), None);
    }
}
