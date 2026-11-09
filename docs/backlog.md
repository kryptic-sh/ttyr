# Backlog

What is not done yet, and why.

- **Phases 03 to 08 are not implemented.** `ttyr::event` (the event types and
  the `Decoder`) is all there is; the rest is planned in `docs/plan/`.
- **The crate name is not reserved on crates.io.** `ttyr` was free on
  2026-11-09; it is taken when the first version is published by the tag
  pipeline in `.github/workflows/ci.yml`.
- **Consumers not surveyed**: gpur, pikr, sqeel, hodl and the other ratatui
  apps' use of crossterm (01's Consumers lists tmxr's and hjkl's).

## Left from phase 02 (events and decoder)

- **No real-terminal byte captures.** Phase 02 planned fixtures captured from
  Alacritty, kitty, foot, Ghostty, WezTerm, xterm, Windows Terminal and the
  Linux console for one key set. The decoder tests use the kitty spec's
  sequences, the crossterm fork's test inputs and tmxr's e2e sequences, written
  by hand; neither tmxr nor hjkl had captures on disk. Taking them needs someone
  at each terminal with a capture tool (`cat -v` or a small reader that prints
  bytes), with kitty flags 1, 3 and 31 pushed.
- **No cargo-fuzz target.** It needs nightly and `libfuzzer-sys`, a new
  dependency and the owner's call. In its place
  `noise_never_panics_and_reads_the_same_in_any_chunks` (in
  `src/event/decoder/tests.rs`) feeds seeded noise of short inputs in random
  chunks: no panic, and the same events however the input is split. It does not
  reach the length caps (`MAX_CSI`, `MAX_STRING`); a fixed test covers
  `MAX_CSI`.
- **Not decoded yet**, each passed on as `Event::Unknown`: DECXCPR
  (`CSI ? row ; col R`), in-band resize (mode 2048, `CSI 48 ; … t`), the
  `CSI 6 t` and `CSI 8 t` size replies, OSC 52 clipboard replies, UTF-8 mouse
  (1005), and mouse buttons 10 and 11. Add each when a consumer asks.
- **No convenience methods on `Event`** (crossterm's `is_key_press`,
  `as_key_event` and the like). Add the ones a port turns out to need.

## Decided in phase 02, where the bytes are ambiguous

So the next surprise is cheap; each is a decoder choice, not a bug.

- **`CSI 1 ; mods R` is a cursor position report**, not xterm's modified F3
  (crossterm reads it the same way). kitty sends F3 as `CSI 13 ~` to avoid this.
- **In the legacy forms (`CSI 1 ; mods A`, `CSI n ; mods ~`, SS3), modifier bit
  8 is Super**, as kitty sends those forms, not xterm's Meta. Only
  `CSI 27 ; mods ; key ~` is xterm's alone and reads bit 8 as Meta.
- **xterm's `formatOtherKeys` (`CSI key ; mods u`) is indistinguishable from
  kitty's**, so it decodes with `KeyEncoding::Kitty`.
- **Alt+\[ typed with a final byte in the same read** (`ESC [ a`) decodes as a
  CSI sequence (`CSI a`, rxvt's Shift+Up), as in crossterm. `ESC [` alone at a
  flush is Alt+\[.
- **`ESC ] digits` and `ESC P >` wait for the rest of a string**, even at a
  flush, so a colour answer or XTVERSION split across reads is not misread. A
  byte that cannot continue the string (not a digit before `;` in OSC, not `|`
  after `ESC P >`) makes the `ESC` Alt+that key instead, and the typing after it
  is kept, where tmxr's fork dropped it.
- **A modifier key's own press does not add itself to `modifiers`** (crossterm
  adds it): the modifiers are as the terminal reported them.
- **A kitty event kind other than 1, 2 or 3 is `Event::Unknown`**, not a press.
- **An upper-case letter from a legacy terminal is its lower case with Shift**,
  the letter in `shifted`, as crossterm adds Shift; Caps Lock typing looks the
  same, as a legacy terminal cannot tell them apart.
- **SGR-pixels mouse (1016) reads as cells**: the same form, so the caller who
  turned it on interprets `column` and `row` as pixels.

## Considered and declined

- **Ctrl+H as Backspace in `KeyEvent::normalize_legacy`.** Dropped on hjkl's
  review (2026-11-09): hjkl runs the helper on every key, and its default keymap
  binds Normal-mode `<C-h>` to focus-left (`nnoremap <C-h> <C-w>h` is one of the
  commonest vim mappings), so a legacy terminal's ^H would stop navigating. The
  decoder already reads DEL as Backspace and ^H as Ctrl+H. A consumer that wants
  the alias gets a separate opt-in function, not a change to `normalize_legacy`.
- **`#[non_exhaustive]` on `KeyCode` and `MouseEventKind`** (raised by hjkl as
  optional): left off, as in crossterm, so a port's exhaustive matches keep
  compiling. ttyr is pre-1.0, so a later variant is a minor bump; revisit at
  1.0.
