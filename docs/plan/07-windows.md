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

Measured on Windows 11 26300 (2026-11-13), with a scratch reader and no person
at the keyboard: the suite passes natively (`x86_64-pc-windows-msvc`); conhost
started directly; and the system ConPTY with tmxr 0.15 as the outer terminal,
keys from `tmxr send-keys` and pastes from `paste-buffer -p`.

- conhost answers DA1 itself, as text records (no virtual-key or scan code);
  ConPTY passes a query out and writes the outer terminal's answer back the same
  way. Both read as `Report`s with VT input off, on, and with win32-input-mode
  on. conhost ignores `?9001`.
- Through ConPTY: VT input with bracketed paste gives one `Event::Paste`. With
  win32-input-mode on as well, the paste stays bracketed, but ConPTY sends its
  line breaks as Enter's records, which the decoder now reads back as `\r`. With
  VT input off a paste is keystrokes. ConPTY honours `?9001` itself.
- Through ConPTY with VT input on, kitty flags are pushed and queried, and keys
  arrive kitty-encoded (the outer terminal, tmxr, implements kitty).
- conhost reports its font size, so pixels; ConPTY reports none.

Then with keystrokes from `SendInput`, which go through the keyboard layout and
the terminal's own key handling as a keyboard's do (Windows Terminal from
`wt.exe -w new`, US layout):

- Windows Terminal: a two-line paste is one `Event::Paste("hi\rEND")` with VT
  input and bracketed paste, and the same with win32-input-mode on as well.
- Windows Terminal does not answer a kitty flags query, and its keys stay in the
  legacy encoding with kitty flag 1 pushed (Ctrl+I reads as Tab). Only
  win32-input-mode tells such keys apart there.
- conhost: real keystrokes arrive as console records, lone modifiers included,
  and its window losing and regaining focus gives `FocusLost` and `FocusGained`.

On a German layout (added to the input list for the run and removed after):
AltGr+Q and AltGr+E read as `@` and `€` with no modifiers, in the console's
records and in win32-input-mode, Windows Terminal releasing AltGr's right Alt
without `ENHANCED_KEY` (read back as right Alt). An Alt code in Windows Terminal
is text records and no Alt release (see the backlog).

Still to do: nothing measurable from here. IME input (the Japanese IME on the
test machine) was not driven.
