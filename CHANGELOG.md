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
- `ttyr::reader::Reader` (Unix): reads events from `/dev/tty` or a given fd with
  crossterm's `poll(timeout)` and `read()`. It delivers `Event::Resize` on
  `SIGWINCH`, watched from before `open` returns, and `Event::Wake` when its
  `Waker` is woken from another thread. Signals never end a wait early or
  surface as errors. An optional escape timeout holds a lone `ESC` for the rest
  of a sequence. Only one reader can be open per process at a time, and it reads
  only when asked to.
- `ttyr::terminal` (Unix): `size` and `window_size` (cells and pixels) of the
  controlling terminal, and `enable_raw_mode`, `disable_raw_mode` and
  `is_raw_mode_enabled`, crossterm's names. Raw mode keeps the attributes from
  before and puts exactly those back; turning it on or off twice does nothing
  the second time.
- `ttyr::command`: crossterm's `Command` trait, `QueueableCommand`,
  `ExecutableCommand` and the `queue!` and `execute!` macros, writing commands
  into any `io::Write`.
- The terminal modes as commands in `ttyr::terminal`, under crossterm's names:
  `EnterAlternateScreen`, `EnableMouseCapture`, `EnableBracketedPaste`,
  `EnableFocusChange`, `PushKeyboardEnhancementFlags`,
  `PopKeyboardEnhancementFlags`, `BeginSynchronizedUpdate` and their offs. New
  ones as well: `SetKeyboardEnhancementFlags` (kitty's set,
  `CSI = flags ; 1 u`), `SetMouseTracking` (one level of mouse reports: 1000,
  1002 or 1003, with or without 1006), `Enable`/`DisableModifyOtherKeys`,
  `Enable`/ `DisableColorSchemeReports` (2031) and `RequestColorScheme`.
  `EnableMouseCapture` turns on 1000, 1002, 1003 and 1006, but not crossterm's
  urxvt 1015: a terminal without SGR reports falls back to X10's form, which the
  decoder reads.
- A `serde` feature deriving `Serialize` and `Deserialize` on every event type.
