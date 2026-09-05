# Changelog

## Unreleased

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
