# Changelog

## Unreleased

- `EditorOutput::caret_rect`: where the caret is on screen, for a popup hung off it.

- `EditorRequest::underlines` underlines stretches of the text in the caller's colours - what a
  language server found wrong - and `EditorOutput::pointed_at` and `pointed_word` say what the
  pointer is over without a modifier held, for a tooltip about it.

- `EditorOutput::word_at_caret`: the word the caret sits in, for a caller asking about a name
  from the keyboard or a menu the way `navigated_to` is from a click.

- `Editor::replace_ranges` puts edits made outside the text area into the text - a rename
  across a project - and carries the caret and the selection through them, so what is typed
  next goes where it would have gone. `set_text` stays the way to load a different text.

- A right-click puts the caret where it was made, unless it was made inside the selection, so
  a context menu opened on a name is about that name.

- Rhai is highlighted. syntect bundles no Rhai grammar and the ones rhaiscript publishes are
  MPL-2.0 tmLanguage files, so `grammars/Rhai.sublime-syntax` is written for this crate, under
  its licence, in the scope names the theme already reads. The code inside a backtick string's
  `${ }` reads as code rather than as more of the string, and block comments nest the way Rhai's
  do.

- Tab indents by one level rather than typing a tab character, and shift-tab takes one level
  off. What a level is is the caller's to say — `EditorRequest::indent`, four spaces until it
  says otherwise, or a tab for a repo written in tabs — because how a file is indented is a
  fact about the repo it belongs to. Anything selected indents every line it touches, so a
  block moves in and out together; the `serde` feature puts `Indent` in an application's own
  settings file.

- TypeScript, TSX and JSX are highlighted. syntect bundles no TypeScript grammar of any kind, so
  a `.ts` file used to be a page of grey text in an editor whose whole point is code; Microsoft's
  TypeScript and TypeScriptReact grammars are now vendored under `grammars/` (Apache-2.0) and
  folded into syntect's own set by a build script, which parses the YAML once at build time
  rather than costing two seconds the first time a `.ts` file is opened. `.jsx` is read with the
  TSX grammar, and `.mjs` and `.cjs` with the JavaScript one, through a table of the extensions
  no grammar claims. Tags, tag attributes, enum members and the brackets TypeScript files under
  its own scope names now land somewhere a theme has an opinion about.

- A list of things to finish the word being typed with, drawn under the caret — or over it when
  the rows would run off the bottom of the screen. The caller fills it with `Completion`s and is
  told the word being typed, which one was taken and whether the list was put away; the editor
  draws the rows, holds which one the keyboard is on, and puts the chosen one into the text.
  While the list is showing it takes the arrows, Enter, Tab and Escape off the text underneath
  it, and nothing else — typing still lands in the buffer.

- The word under the pointer while a modifier is held, underlined and offered with a
  pointing-hand cursor, and reported when it is clicked: what a caller with somewhere to send a
  name — a definition, a symbol index, a language server — hangs go-to-definition off.
  `EditorRequest::navigate_modifier` says which modifier, since which one it is is a platform
  convention rather than the widget's business.
- Where the caret sits, as a byte offset with the line and column it is on, so a caller can show
  a reading of it or ask about the place it is at.

## 0.1.0

First release. A code editor widget: a text buffer it owns, a line-number fringe that scrolls
with the code, marks drawn into the text, and select-a-range-and-say-where-it-landed.
