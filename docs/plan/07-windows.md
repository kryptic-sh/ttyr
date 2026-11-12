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

Still to do:

- **Run it by hand in Windows Terminal**: a paste with
  `Mode::VirtualTerminalInput`, `Mode::Win32Input` and `Mode::BracketedPaste`
  (hjkl's planned modes), AltGr characters on a non-US layout, whether kitty
  flags do anything in Windows Terminal itself, and an Alt code typed on the
  keypad. In conhost: focus records when the window loses and regains focus.
