# 02 — Events and the decoder

A pure decoder from bytes to events, with no I/O, so every terminal's bytes can
be tested directly.

## Event types

- **Key**: the key (the unshifted key the kitty protocol reports), the shifted
  key and base-layout key where reported, modifiers (Shift, Alt, Ctrl, Super,
  Hyper, Meta), lock states (Caps Lock, Num Lock), the event type (press,
  repeat, release), the associated text, and keypad-versus-main distinctions.
  Which encoding it came in (legacy, xterm, kitty) is kept, so a consumer can
  tell what the terminal could and could not have reported.
- A cheap constructor for hand-built events (tests, macro replay).
- **Mouse**: SGR (1006) and the legacy X10/UTF-8 forms; down, up, drag, moved,
  scroll in four directions; button, modifiers, position; SGR-pixels (1016)
  where asked.
- **Paste** (bracketed paste, 2004), **focus** gained and lost (1004),
  **resize**.
- **Reports**: the kitty flags query reply (`CSI ? flags u`), DA1, DA2,
  XTVERSION, DECRQM replies (synchronized output 2026 among them), the
  colour-scheme reports (`CSI ? 997 ; n n`), OSC 4/10/11/12 colour replies,
  cursor position (CPR). Unknown well-formed reports become an "unknown report"
  event rather than errors that swallow later input (crossterm#1106 in tmxr's
  fork).
- `serde` support behind a feature (tmxr sends events between processes).

## Helpers for the key that was meant

The raw event stays lossless; these are opt-in views on it, so tmxr, hjkl and
the rest share one rule instead of each writing its own (both wrote one and both
got Alt+Shift+a wrong as M-a, 2026-11).

- **`logical_char()`** (name to settle): the associated text if the terminal
  sent it; else the shifted key where Shift was held and the terminal reported
  it (kitty flag 4); else, from a terminal that reported only the unshifted key
  (kitty flag 1, xterm, legacy), the uppercase of an ASCII letter. Shift is
  cleared only where the returned character already carries it; otherwise it
  stays the only record that Shift was held (Alt+Shift+1 under flag 1).
- **Legacy aliases** (what `hjkl_kitty::normalize_legacy` does today): Ctrl+[ as
  Esc, Ctrl+I as Tab, Ctrl+M as Enter, Ctrl+H as Backspace from terminals that
  send ^H for it. Correct only if the event's encoding field is: under kitty
  flag 1 those are distinct keys and must stay so.
- Tests cover each rule per encoding, including the M-A case from each.

## The decoder

- **kitty `CSI … u`**, every flag: 1 disambiguate, 2 event types, 4 alternate
  keys (`key:shifted:base`), 8 all keys as escape codes (including the
  functional keys' `CSI … u` and `CSI … ~` forms and the modifier keys
  themselves), 16 associated text (`; text-as-codepoints`).
- **xterm modifyOtherKeys** `CSI 27 ; mods ; key ~` and its `formatOtherKeys`
  `CSI key ; mods u` form; modifier parameters with Meta (xterm's bit 8).
- **Legacy**: C0 controls, `ESC` + key as Alt (including Alt+Esc as `ESC ESC`,
  tmxr fork), SS3 and CSI cursor and function keys with xterm modifier
  parameters, `CSI Z`, rxvt and linux-console function keys.
- **The Escape ambiguity**: a lone `ESC` is Escape only after a timeout with
  nothing following; the timeout is the caller's (tmux's `escape-time`), not a
  constant in ttyr.
- **Partial input**: a sequence split across reads is held, never misread; a
  burst longer than one read is delivered whole (crossterm#1155 in the fork).
- UTF-8 text, including invalid bytes handled without losing what follows.

## Tests

- The kitty spec's examples, each a test naming its section.
- Byte captures from Alacritty, kitty, foot, Ghostty, WezTerm, xterm, Windows
  Terminal and the Linux console, for the same key set, stored as fixtures.
- Every fix in tmxr's crossterm fork, as a regression test.
- A fuzz target over the decoder (no panic, no lost bytes after garbage).
