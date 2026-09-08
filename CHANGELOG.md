# Changelog

## Unreleased

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
