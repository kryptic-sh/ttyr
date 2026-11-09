# 07 — Windows

- **Input**: the console hands over input records (`ReadConsoleInputW`), not
  bytes. Map them to 02's events: key down and up (so release events exist here
  without the kitty protocol), mouse, focus, buffer resize. With
  `ENABLE_VIRTUAL_TERMINAL_INPUT` the console instead passes VT bytes through,
  which 02's decoder reads; Windows Terminal's win32-input-mode (`CSI ? 9001 h`)
  sends records as escape sequences. Which source a program gets is its choice;
  tmxr already distinguishes Windows Terminal (`WT_SESSION`) from ConPTY behind
  another terminal.
- **Raw mode**: console modes (`SetConsoleMode`), restoring what was found.
- **Output**: `ENABLE_VIRTUAL_TERMINAL_PROCESSING`, so 05's sequences work
  unchanged.
- **Size**: `GetConsoleScreenBufferInfo`.
- **Not known yet**: whether kitty keyboard sequences survive ConPTY at all
  (tmxr's plan 18 expects Windows panes to stay at kitty flag 1 or below).
- CI runs the tests on windows-latest from the first commit; code under
  `cfg(windows)` is only checked there.
