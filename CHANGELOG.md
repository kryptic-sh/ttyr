# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `ttyr::event`: the event types (`Event`, `KeyEvent`, `KeyCode`,
  `KeyModifiers`, `KeyEventState`, `KeyEventKind`, `MouseEvent`, `Report`) with
  crossterm's names where the meaning is the same. `KeyEvent` has no `code`: it
  has `key` (the unshifted key), `shifted`, `base_layout`, `text` and
  `encoding`, so a port from crossterm fails to compile at each `.code` instead
  of silently not matching.
- `ttyr::event::Decoder`, a pure decoder from terminal bytes to events: the
  kitty keyboard protocol with every flag, xterm's modifyOtherKeys, legacy keys
  (with rxvt's and the Linux console's), SGR, X10 and urxvt mouse, bracketed
  paste, focus, and the reports (kitty flags, DA1, DA2, XTVERSION, DECRPM, the
  colour scheme, OSC 4/10/11/12 colours, cursor position, text-area pixels).
  Unknown sequences and invalid bytes arrive as `Event::Unknown` rather than
  swallowing what follows them. When a lone `ESC` counts as Escape is the
  caller's choice (`Decoder::flush_event`).
- `KeyEvent::logical_char` and `KeyEvent::logical_modifiers`, the character a
  key was meant to type across encodings, and `KeyEvent::normalize_legacy`,
  hjkl-kitty's Ctrl+\[/I/M aliases.
- A `serde` feature deriving `Serialize` and `Deserialize` on every event type.
