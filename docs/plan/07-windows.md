# 07 — Windows

**In progress.** Landed, all compile-checked locally and tested in CI on
windows-latest:

- The console's key and mouse records as events (`event::console`, plain values,
  so their tests run on every platform), and Windows Terminal's win32-input-mode
  decoded through the same mapping.
- `Reader::open` on the console. It waits on the input handle and the waker's
  event together, and takes records or, with VT input on, the terminal's bytes.
- Raw mode, size and VT output processing (`terminal::enable_raw_mode` and the
  rest, the same names as on Unix).
- The guard on Windows: `Mode::Mouse` also sets `ENABLE_MOUSE_INPUT`,
  `Mode::VirtualTerminalInput` sets `ENABLE_VIRTUAL_TERMINAL_INPUT`, and
  `Mode::Win32Input` writes `CSI ? 9001 h`. `suspend` is Unix only (no job
  control), and `suspend_with` works on both.

Still to do:

- **Run it on a real Windows machine**: Windows Terminal (records, VT input and
  win32-input-mode), conhost, and ConPTY behind another terminal. The owner
  plans a session this week.
- **Not known yet**: whether kitty keyboard sequences survive ConPTY at all
  (tmxr's plan 18 expects Windows panes to stay at kitty flag 1 or below).
