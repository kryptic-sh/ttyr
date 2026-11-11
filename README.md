# ttyr

Terminal input, output and raw mode for Rust, with the full kitty keyboard
protocol and a ratatui backend: kryptic-sh's replacement for crossterm, shared
by [tmxr](https://github.com/kryptic-sh/tmxr),
[hjkl](https://github.com/kryptic-sh/hjkl) and the other kryptic-sh terminal
apps.

**Status: in progress.** Implemented: the event types and the byte decoder
(`ttyr::event`), reading a Unix tty or the Windows console (`ttyr::reader`),
crossterm's `Command` with `queue!` and `execute!` (`ttyr::command`), the output
commands (`ttyr::cursor`, `ttyr::style`, `ttyr::terminal`), the terminal's size,
raw mode, modes, restore guard and capability queries (`ttyr::terminal`), and a
ratatui backend (`ttyr::backend`, with the `ratatui` feature). Windows has not
run on a real machine yet; what is left is in
[docs/plan](docs/plan/00-index.md).

## Porting from crossterm

Most names are crossterm's where the meaning is the same. What a port will look
for and not find, on purpose:

- **`event::poll` and `event::read` as free functions.** They are methods on
  `ttyr::reader::Reader`, which a program opens once (`Reader::open`) and keeps.
- **`KeyEvent.code`.** A key has `key` (the unshifted key), `shifted`,
  `base_layout` and `text`; `logical_char()` is the character meant, and
  `chord()` is what a keymap compares.
- **`cursor::position()` and `supports_keyboard_enhancement()`.** Both write a
  query and block until the terminal answers, up to two seconds on Unix, then
  fail: a stall at startup, or in a test harness, on a terminal that never
  answers. ttyr never waits on the terminal: write `RequestCursorPosition` or
  `RequestKeyboardEnhancementFlags` (with `RequestPrimaryDeviceAttributes` last)
  and read the answers as `Event::Report`, or with
  `Reader::collect_answers(timeout)`.
- **`terminal::SetSize`** (`CSI 8 ; rows ; cols t`). Not provided; a program
  that needs it writes the sequence itself with `style::Print`.
- **`Command::execute_winapi` and `is_ansi_code_supported`.** ttyr writes
  sequences on Windows too, with VT processing turned on by raw mode, so a
  `Command` has only `write_ansi`; delete the other two from your own
  implementations.

## License

MIT
