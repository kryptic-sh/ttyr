# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- Windows: Ctrl with an OEM key the console gives no character for (Ctrl+;,
  Ctrl+, Ctrl+. and the rest) is now that key, its character on a US layout,
  where before the key was dropped. In the console's records and in
  win32-input-mode alike.
- `Reader` over a non-blocking fd (`Reader::from_fd` with `O_NONBLOCK` set, as
  some async runtimes leave stdin) no longer fails `poll` and `read` with
  `WouldBlock` when the tty has nothing to read after all; the wait goes on.
- A kitty `CSI u` key with an empty text parameter, or an empty value in it
  (`CSI 97 ; 1 ; u`, `CSI 97 ; ; u`), is the key with no text instead of
  `Event::Unknown`.
- A bracketed paste whose end marker never comes is no longer held for ever:
  past 16 MiB without one, the decoder gives it up as `Event::Unknown` (its
  start marker and the bytes held) and reads what follows afresh, so memory
  stays bounded and the input recovers.
- `Reader::poll`, `read` and `collect_answers` no longer panic when bytes are
  held under an escape timeout too long to add to the time
  (`set_escape_timeout(Some(Duration::MAX))`): such a timeout never reads a lone
  `ESC` as typed.

## [0.1.0] - 2026-11-11

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
  Modifier bit 8 is Super while the kitty protocol is on and xterm's Meta
  otherwise (`Decoder::set_kitty_keyboard`, `Reader::set_kitty_keyboard`, also
  set by the terminal's answer to a flags query). In xterm's forms an upper-case
  letter without Shift is Caps Lock, so Ctrl+h stays Ctrl+h. Unknown sequences
  and invalid bytes arrive as `Event::Unknown` rather than swallowing what
  follows them. When a lone `ESC` counts as Escape is the caller's choice
  (`Decoder::flush_event`).
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
- The restore guard in `ttyr::terminal` (Unix):
  - `enable` and `disable` turn a `Mode` on and off and record it. `Mode` covers
    the alternate screen, mouse at one level, bracketed paste, focus, colour
    scheme reports, modifyOtherKeys and kitty flags. Enabling a mode that is on
    with another value replaces it.
  - `restore` turns every recorded mode off in reverse order, shows the cursor
    and leaves raw mode. A second call does nothing.
  - `restore_on_panic` installs a panic hook, once, that restores and then runs
    the hook that was there before.
  - `suspend_with(out, f)` gives the terminal back while `f` runs, then takes it
    again; `suspend` stops the process with `SIGTSTP` in between.
- Capability queries as commands in `ttyr::terminal`: they write the question
  and never wait for the answer, which arrives through the reader as
  `Event::Report`. `RequestKeyboardEnhancementFlags` (`CSI ? u`),
  `RequestPrimaryDeviceAttributes` (DA1), `RequestSecondaryDeviceAttributes`
  (DA2), `RequestTerminalVersion` (XTVERSION), `RequestMode` (DECRQM),
  `RequestTerminalColor` (OSC 10/11/12/4) and `RequestTextAreaPixels`
  (`CSI 14 t`).
- `Reader::collect_answers(timeout)` reads the answers to queries, until DA1's
  answer or the timeout. It returns them as `Answers`: every event read
  meanwhile in the order it came (keys typed, a resize and a wake among the
  reports), with `reports()` and `others()` to take either apart, and whether
  DA1's answer came.
- `ttyr::cursor`: crossterm's cursor commands (`MoveTo`, `MoveToColumn`,
  `MoveToRow`, `MoveUp`/`Down`/`Left`/`Right`, `MoveToNextLine`,
  `MoveToPreviousLine`, `SavePosition`, `RestorePosition`, `Hide`, `Show`,
  `EnableBlinking`, `DisableBlinking`, `SetCursorStyle`), plus `SetCursorColor`
  and `ResetCursorColor` (OSC 12/112) and `RequestCursorPosition`, whose answer
  arrives as an event rather than being waited for.
- `ttyr::style`: crossterm's `Color`, `Colors`, `Attribute` (with the curly,
  dotted and dashed underlines and overline), `SetForegroundColor`,
  `SetBackgroundColor`, `SetUnderlineColor`, `SetColors`, `SetAttribute`,
  `ResetColor` and `Print`, writing crossterm's bytes. `NO_COLOR` is the
  program's to honour.
- Screen commands in `ttyr::terminal`: `Clear` (crossterm's `ClearType`s, plus
  `FromLineStart`), `ScrollUp`, `ScrollDown`, `SetScrollRegion`,
  `ResetScrollRegion`, `DisableLineWrap`, `EnableLineWrap` and `SetTitle`.
- The decoder reads Windows Terminal's win32-input-mode key records
  (`CSI Vk ; Sc ; Uc ; Kd ; Cs ; Rc _`) as key events with the new
  `KeyEncoding::Windows`. Releases and repeats are reported, and Ctrl+Enter and
  Ctrl+Backspace keep their Ctrl. An AltGr character has no Ctrl+Alt, and Caps
  Lock, Num Lock and the keypad are in the state. The same mapping will read the
  console API's records on Windows. An Alt code's character is a press followed
  by its release. A record with no virtual-key or scan code (conhost's answers
  to queries) is read as the bytes it carries, so a DA1 answer arrives as a
  report, not as typing.
- Windows (`windows-sys`):
  - `Reader::open` reads the console. It waits on its input and the `Waker`'s
    event together, so a wake ends a wait as on Unix.
  - Key and mouse records read as events, the mouse's back and forward buttons
    included. A resize reads as `Event::Resize` with the window's size.
  - With VT input on, the bytes the terminal sends are decoded as on Unix.
  - With VT input off, focus records read as `Event::FocusGained` and
    `Event::FocusLost`, as in crossterm. With it on they are dropped: the
    terminal reports focus itself under `EnableFocusChange`.
  - `terminal::size`, `window_size`, `enable_raw_mode` and `disable_raw_mode`
    work through the console API. Raw mode also turns on VT output processing.
  - The guard sets the console's input modes a mode needs, and turning the mode
    off puts back the bits the console had before, as `restore` and the panic
    hook do. There are new modes: `Mode::Win32Input` (`CSI ? 9001 h`, with the
    commands `EnableWin32InputMode`/`DisableWin32InputMode`), and
    `Mode::VirtualTerminalInput`, Windows only.
  - `terminal::suspend` stays Unix only; `suspend_with` works on both.
- A `ratatui` feature with `ttyr::backend::TtyrBackend`, a drop-in for
  ratatui-crossterm's `CrosstermBackend`. It writes the same bytes for the same
  buffers, tested side by side, and it is an `io::Write`, so commands go through
  it in order with the frames. `get_cursor_position` returns where the backend
  last put the cursor instead of asking the terminal and waiting.
- `execute!` calls `io::Write::flush` explicitly, so it works on a writer that
  is also a ratatui backend (which has a `flush` of its own).
- Colour answers (OSC 4/10/11/12) are read in each of Xlib's numeric forms:
  `rgb:` with 1 to 4 hex digits a channel, `#` with 3, 6, 9 or 12 digits, and
  `rgbi:`. Before, only `rgb:` was read, and a terminal answering otherwise lost
  its colours.
- `terminal::enable_raw_mode_on(fd)`, `size_of(fd)` and `window_size_of(fd)`
  (Unix): raw mode and the size on a given terminal, such as stdin, for a
  program with no controlling terminal (a `setsid` launcher, some service
  managers). `disable_raw_mode`, the guard's `restore` and its panic hook act on
  that terminal.
- A `serde` feature deriving `Serialize` and `Deserialize` on every event type.

[Unreleased]: https://github.com/kryptic-sh/ttyr/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/kryptic-sh/ttyr/releases/tag/v0.1.0
