# Backlog

What is not done yet, and why.

- **Phase 08 is not done, and 07 is not finished**: moving the consumers off
  crossterm, and Windows on a real machine, planned in `docs/plan/`.
- **Consumers not surveyed**: gpur, pikr, sqeel, hodl and the other ratatui
  apps' use of crossterm (01's Consumers lists tmxr's and hjkl's).
- **The Windows work under `[Unreleased]` waits on the owner's call to
  release.** It changes what existing keys produce on Windows (Shift+symbol as
  its key with the character shifted, lone modifier and lock keys as events, an
  Alt code's closing Alt release), so the proposal is 0.3.0, not a patch. tmxr
  (pins ttyr 0.2 from crates.io) takes it after running its Windows suite
  against it; hjkl waits on it to turn on `Mode::VirtualTerminalInput` and
  `Mode::Win32Input` with bracketed paste. Both asked to be told the version
  once it is on crates.io.

## Left from the 0.1.0 review

The 0.1.0 review's findings (2026-11-11) were fixed or decided in the commits
after it; what remains is below: two bugs that need the owner's call, one held
as a decision, and the coverage that is still missing or cannot be tested.

### Bugs

- **A bracketed paste whose end marker is lost still swallows the keys typed
  after it, up to the paste cap.** `Decoder::paste` (`src/event/decoder.rs`) now
  gives a paste up as `Event::Unknown` once it passes `MAX_PASTE` with no
  `CSI 201 ~`, which bounds memory and makes recovery certain, but a person
  typing after a lost end marker (the program wrote `DisableBracketedPaste`
  mid-paste, a terminal or ConPTY dropped the marker) never types that much, so
  in practice their keys still vanish until the `Reader` is dropped. Recovering
  sooner is a decision: new API (`Decoder::reset` and `Reader::reset`, which a
  caller uses after its own timeout; additive, and the next release is 0.2.0
  anyway), or a policy in `Reader::wait` that gives a paste up when nothing has
  arrived for some time (no new API, but a paste a slow ConPTY delivers in
  pieces would then be cut in two). crossterm's parser waits the same way, as
  recalled, not checked.
- **A DA2 or DA1 answer with a value above 65535 decodes as `Event::Unknown`.**
  `flat` (`src/event/decoder/csi.rs`) narrows each parameter to `u16`; a
  terminal whose DA2 version field is larger loses the whole report (foot, for
  one, packs major, minor and patch into one number; whether any shipped
  terminal's value exceeds `u16` was not checked). Verified by running with a
  5-digit version. Fix: `Vec<u32>` in `Report::SecondaryDeviceAttributes` and
  `PrimaryDeviceAttributes`, a public type change, not made in the 0.1.0
  review's fixes for want of the owner's approval; the next release is 0.2.0, so
  it costs no extra break if made before it.
- **A truncated UTF-8 lead byte is held across a flush, delaying the next key.**
  `parse_utf8` (`src/event/decoder.rs`) ignores `at_end`, so a lone 0xC3 (noise
  on a serial line, a terminal that dropped a byte) stays pending through
  `flush_event`; the next key arrives as `Event::Unknown([0xC3])` followed by
  the key, after a delay of one keystroke. `incomplete_utf8_waits` asserts the
  hold, so it is a decision; verified by running. The alternative is to give a
  lead byte up at a flush, which misreads a character split across the
  terminal's writes when the reader has no escape timeout. Record which is
  wanted.

### Test coverage

- **The `serde` feature has no test.** Nothing serializes an `Event` and reads
  it back; `cargo test --all-features` only compiles the derives. tmxr sends
  `Event`s between its client and server with it. A round trip needs a format
  crate as a dev-dependency (`serde_json` or the `postcard` tmxr uses), the
  owner's call.
- **The Unix reader's interrupt paths are untested, and cannot be driven
  deterministically.** `EINTR` from `select` and from `read`, and the
  `Interrupted` arm of `drain` (`src/reader/unix.rs`), need a signal to land
  inside that one system call. A test that raises a signal and hopes would pass
  whether or not the arm ran, so none was written. A full wake pipe
  (`WouldBlock` in `WakeHandle::wake`) is tested
  (`wakes_that_fill_the_pipe_are_one_event`), and so are the Windows
  `Source::wait` failure arms (`a_wait_that_fails_is_an_error`,
  `an_unexpected_wait_result_is_an_error`).
- **`AllocConsole` failed once in `without_a_console`**
  (`src/terminal/guard/tests/windows.rs`), on the first native Windows run
  (2026-11-13, `x86_64-pc-windows-msvc`, Windows 11 26300): 143 passed, and
  `a_console_that_fails_still_writes_the_off_sequences_on_restore` failed at the
  assert after `AllocConsole`. 28 runs after it passed, the standard-handle
  restore included. The asserts now print the OS error; keep it if it happens
  again.

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

## Left from phase 03 (Unix reader)

- **No async `Stream`.** Neither consumer needs one (01's open question on
  async); add it behind a feature when one does.
- **Not verified on a real terminal**, only over a pty in the tests: `/dev/tty`
  as the controlling terminal, and a `SIGWINCH` the kernel sends on a real
  resize (the tests raise it themselves, as a pty that is not the controlling
  terminal gets none). tmxr's adoption is the first real run.
- **macOS and the BSDs run only in CI.** `Reader` waits with `select` for
  macOS's sake; the tests ran locally on Linux.
- **The `unsafe` `select` call has had no dynamic analysis.** Miri cannot run
  system calls; the safety argument is that every fd in the set is borrowed from
  the `Reader` for the call (see `Reader::wait_ready`).
- **A spinning wait shows only as a hang.** If the `flushed` guard in
  `Reader::wait` were lost, a held paste or long sequence would be flushed again
  and again, never reaching the deadline:
  `a_paste_waits_past_a_flush_for_its_end` then hangs rather than fails (checked
  by removing the guard).

## Left from phase 06 (ratatui backend)

- **No `scrolling-regions` support.** ratatui-core puts `scroll_region_up` and
  `scroll_region_down` behind its own `scrolling-regions` feature, and a backend
  has to implement them once any crate in the build turns it on. hjkl uses
  ratatui's default features, without it. Adding it takes a `scrolling-regions`
  feature in ttyr that forwards to ratatui-core's, as ratatui-crossterm has,
  plus the two methods on `SetScrollRegion`.
- **`get_cursor_position` doesn't ask the terminal**: it returns where
  `TtyrBackend` last put the cursor, one column after the start of the last cell
  drawn (a wide cell leaves it a column short). ratatui calls it in
  `Terminal::clear`, to put the cursor back, and for inline viewports, which
  hjkl doesn't use. An inline viewport that needs the real position would write
  `RequestCursorPosition` and read the answer itself.
- **Underline colour is always on** (`ratatui-core/underline-color`, which
  ratatui's default features turn on too), as `CrosstermBackend` has it by
  default.

## Unexplained

- **One `cargo test --all-features` run failed once and did not again in 43
  runs** (2026-11-10, load average around 12 from other sessions' builds). Which
  test it was was not kept; the reader's pty tests wait on timing and are the
  likeliest. Keep the failing test's name if it happens again.

## Phase 07 in progress (Windows)

- **Windows Terminal's Alt code is text with no release**: it keeps the keypad
  digits and Alt's release, and sends the character as text records (no
  virtual-key or scan code), so `é` from Alt+0233 reads as a legacy key with no
  release (measured), where conhost's Alt code is a Windows key pressed and
  released. Alt's press is released by the next key record, whose state has no
  Alt (`ConsoleKeys::stale_modifiers`); with no key after it, Alt stays down.
- **Focus records are read only with VT input off**, as crossterm reads them,
  although Microsoft documents `FOCUS_EVENT` as the console's internal. With VT
  input on they are dropped, so focus is reported once, by the terminal under
  mode 1004. conhost sends a gained record when the program starts, in both
  input modes, and lost and gained records when its window loses and regains
  focus (measured with VT input off).
- **A key record with no virtual-key and no scan code is text**
  (`KeyRecord::is_text`), read as a terminal's bytes, in win32-input-mode and in
  the console's records. Measured: conhost writes its own DA1 answer
  (`CSI ? 61 ; 1 ; 6 ; … c`) into the input so, and ConPTY writes the outer
  terminal's answers so, with VT input off, on, and with win32-input-mode on;
  each reads as `Report::PrimaryDeviceAttributes`. A character typed with no key
  behind it (IME, `SendInput` without a virtual key) reads as a legacy key with
  no release either.
- **An Alt code's release is made up**: the console sends its character only
  with Alt's release, so ttyr reports the character's press and, at once, its
  release, then Alt's release.
- **`window_size` has no pixels behind ConPTY**: conhost reports its font (8×16
  Consolas measured, 960×480 for 120×30, matching its client area), but under
  ConPTY `GetCurrentConsoleFontEx` gives a width of 0 and the console window is
  0×0, so Windows Terminal's pixels are 0. A program that needs them there asks
  the terminal (`CSI 14 t`, whose answer ttyr does not decode yet).
- **AltGr pressed alone is left Ctrl and right Alt**: the console sends a record
  for the left Ctrl it makes up for AltGr, so a lone AltGr is
  `Modifier(LeftControl)` and `Modifier(RightAlt)`, pressed and released, not
  kitty's `IsoLevel3Shift`. Telling AltGr's Ctrl from a real one needs the
  record's timing or the layout's AltGr flag (`KLLF_ALTGR`), not read.
- **Ctrl with a non-letter key that has a control code reads the control code**,
  not the layout: on a layout where `[` is not where VK_OEM_4 sits, Ctrl+that
  key still reads as Ctrl+\[ when the console gives 0x1B. Kept on purpose:
  tmxr's client maps Ctrl+- and Ctrl+Shift+2/6/- by these events
  (`ConsoleBytes`), and hjkl binds `<C-]>` and the like by character. Where the
  console gives no control code (Ctrl+; and the other OEM keys) the key is read
  from the layout (`layout_char` in `src/reader/windows.rs`), falling back to
  `us_oem_char` for a dead key or a `Decoder` with no layout.

## Left from phase 05 (output)

- **No hyperlink (OSC 8) or clipboard (OSC 52) commands.** Neither hjkl nor tmxr
  writes them through crossterm (hjkl's `hjkl-clipboard` has its own OSC 52
  code), and OSC 52 needs base64, a new dependency or a hand-written encoder.
  Add them when a consumer asks.
- **No `ContentStyle`, `Stylize` or `Attributes` set** (crossterm's styled
  text). Nothing in hjkl or tmxr uses them directly; 06's backend decides
  whether it needs an attribute set.
- **`NO_COLOR` is not read**, where crossterm's colour commands write nothing
  when it is set: whether to colour is the program's choice. A port that relied
  on crossterm for it checks `NO_COLOR` itself.

## Left from phase 04 (terminal modes)

- **No SGR-pixels mouse (1016) in `SetMouseTracking` or `Mode::Mouse`.** Nothing
  asks for it (tmxr said so). The decoder reads its reports as cells already
  (see Decided in phase 02).

- **`suspend` itself has no test**: raising `SIGTSTP` would stop the test
  process. `suspend_with`, which it is built on, is tested; `suspend` adds only
  the `raise`.
- **A `SIGTSTP` sent from outside while in raw mode** (`kill -TSTP`) stops the
  process with the terminal still raw and its modes on. Ctrl+Z in raw mode is a
  key, not a signal, so a program calls `suspend` itself. Catching an outside
  one needs a handler that restores before stopping.
- **Modes written with the commands directly**
  (`execute!(out, EnableMouseCapture)`) aren't recorded, so `restore` doesn't
  turn them off. A port that wants them restored moves them to `enable`.
- **`restore_on_panic` writes to the terminal, not to `out`**, so bytes the
  program queued in a buffered writer before the panic may arrive after the
  restore, or never.

## Decided in phase 02, where the bytes are ambiguous

So the next surprise is cheap; each is a decoder choice, not a bug.

- **`CSI 28 ~` to `CSI 34 ~` are F15 to F20, as xterm's ctlseqs number them**
  (25 F13, 26 F14, 28 F15, 29 F16, 31 F17 to 34 F20). crossterm 0.29's
  `parse_csi_special_key_code` reads 28 and 29 as F13 and F14 and 31 to 34 as
  F14 to F17, so a port that bound those keys under crossterm sees different
  numbers here.
- **`CSI 1 ; mods R` is a cursor position report**, not xterm's modified F3
  (crossterm reads it the same way). kitty sends F3 as `CSI 13 ~` to avoid this.
- **Modifier bit 8 is Super only while the kitty protocol is on**
  (`Decoder::set_kitty_keyboard`, or the terminal's answer to a flags query);
  otherwise it is xterm's Meta, as tmux reads it. `CSI 1 ; 9 A` is the same
  bytes from both, so a program that pushes kitty flags without saying so gets
  Super as Meta. A kind (`mods:kind`) or a bit above 8 is kitty's either way.
  `CSI 27 ; mods ; key ~` is xterm's alone and always reads bit 8 as Meta.
- **An upper-case letter without Shift in xterm's forms is Caps Lock**:
  `CSI 27 ; 5 ; 72 ~` is Ctrl+h with `CAPS_LOCK` in the state, not Ctrl+Shift+h,
  so binds on Ctrl+h hold with Caps Lock on.
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
- **Exhaustive event enums for crossterm ports** (raised by hjkl as optional,
  first declined so a port's exhaustive matches kept compiling): decided the
  other way by the owner (2026-11-11). Every public enum in `ttyr::event` is
  `#[non_exhaustive]`, as `Event`, `Report` and `MouseButton` already were: ttyr
  is young and still growing keys, buttons and reports, and each would otherwise
  be a breaking change. A port adds a wildcard arm once. `ClearType`,
  `MouseTracking`, `SetCursorStyle`, `Color` and `Attribute` outside the event
  module were left exhaustive.
