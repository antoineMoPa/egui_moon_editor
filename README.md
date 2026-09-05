# egui_moon_editor

A code editor widget for [egui](https://github.com/emilk/egui).

egui's `TextEdit` is a text box. This crate is the rest of what makes an editor:

- a fringe of line numbers, drawn where the laid-out text really put each line, scrolling down
  the page with the code and staying put as the code slides sideways under it
- a page that fills the space it was given, so a short file sits on a page rather than in a box
  the size of the text in it
- marks tinted into the text, with the current one underlined, so a search shows its matches
  whether or not the editor holds the keyboard
- select a range and be told where on screen it landed, scrolled to on the axis that can act
  on it
- the word under the pointer while a modifier is held — underlined, with a pointing-hand cursor,
  and reported when it is clicked — and where the caret sits, in bytes with the line and column
  of it, which is what a caller with somewhere to send a name needs to send it
- a list under the caret of things to finish the word being typed with, keyboard-driven: the
  caller offers the candidates and is told which one was taken, and the editor draws them and
  puts the chosen one into the text

```rust
let style = egui_moon_editor::EditorStyle::from_visuals(ui.visuals());
let output = editor.ui(ui, &style, &egui_moon_editor::EditorRequest::default());
```

The widget owns a buffer and how it is drawn. Where the text came from and where it goes stays
with the caller, and so does anything a find bar would hold — the query, the keyboard, the
tally. `Editor` takes the ranges to tint as per-frame input and hands back how many it laid out
and where the current one landed.

A whole file open in a window:

```sh
cargo run --example edit -- src/lib.rs
```

## License

MIT
