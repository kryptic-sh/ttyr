# Backlog

What is not done yet, and why.

- **Phase 08 is not done, and 07 is not finished**: moving the consumers off
  crossterm, and Windows on a real machine, planned in `docs/plan/`.
- **Consumers not surveyed**: gpur, pikr, sqeel, hodl and the other ratatui
  apps' use of crossterm (01's Consumers lists tmxr's and hjkl's).

## Review findings (0.1.0)

A review of the whole crate as released (2026-11-11), on Linux: every source and
test file read, `cargo test --all-features` run (115 tests and 1 doctest
passed), `cargo llvm-cov --all-features` run (92.68% of lines, 90.63% of
regions; the lowest files `terminal/unix.rs` at 75.34% of lines, `backend.rs`
85.14%, `terminal/guard.rs` 86.92%, `event/console.rs` 87.97%, `reader/unix.rs`
89.01%), and suspected bugs tried in a scratch program against the crate.
Windows code was read, not run. Each entry says whether it was verified by
running something or inferred from reading. Entries are ordered most severe
first within each group.

### Bugs

- **A bracketed paste whose end marker is lost still swallows the keys typed
  after it, up to the paste cap.** `Decoder::paste` (`src/event/decoder.rs`) now
  gives a paste up as `Event::Unknown` once it passes `MAX_PASTE` with no
  `CSI 201 ~`, which bounds memory and makes recovery certain, but a person
  typing after a lost end marker (the program wrote `DisableBracketedPaste`
  mid-paste, a terminal or ConPTY dropped the marker) never types that much, so
  in practice their keys still vanish until the `Reader` is dropped. Recovering
  sooner is a decision: new API (`Decoder::reset` and `Reader::reset`, which a
  caller uses after its own timeout; additive, so a minor bump rather than the
  patch this fix went into), or a policy in `Reader::wait` that gives a paste up
  when nothing has arrived for some time (no new API, but a paste a slow ConPTY
  delivers in pieces would then be cut in two). crossterm's parser waits the
  same way, as recalled, not checked.
- **A DA2 or DA1 answer with a value above 65535 decodes as `Event::Unknown`.**
  `flat` (`src/event/decoder/csi.rs`) narrows each parameter to `u16`; a
  terminal whose DA2 version field is larger loses the whole report (foot, for
  one, packs major, minor and patch into one number; whether any shipped
  terminal's value exceeds `u16` was not checked). Verified by running with a
  5-digit version. Fix: `Vec<u32>` in `Report::SecondaryDeviceAttributes` and
  `PrimaryDeviceAttributes`, a breaking change cheap before adoption.
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
- **The reader's interrupt and failure paths are untested, and cannot be driven
  deterministically.** `EINTR` from `select` and from `read`, and the
  `Interrupted` arm of `drain` (`src/reader/unix.rs`), need a signal to land
  inside that one system call; the Windows `WAIT_FAILED` and unexpected-result
  arms of `Source::wait` (`src/reader/windows.rs`) need the wait itself to fail.
  A test that raises a signal and hopes would pass whether or not the arm ran,
  so none was written. A full wake pipe (`WouldBlock` in `WakeHandle::wake`) is
  tested (`wakes_that_fill_the_pipe_are_one_event`).
- **The Windows tests added since 0.1.0 have not run**: they compile for
  `x86_64-pc-windows-gnu` here and run only in CI. In particular
  `a_console_that_fails_*` in `src/terminal/guard/tests/windows.rs` free the
  test process's console (`FreeConsole`) and allocate a new one, which no test
  did before.

### Code quality

- **Three copies of the "an upper-case letter arrived" rule.** `char_key`
  (`src/event/decoder.rs`), `modified_char_key` (`src/event/decoder/csi.rs`) and
  `char_event` (`src/event/console.rs`) each decide how an upper-case character
  becomes a lower-case `key` with Shift, `shifted` and Caps Lock, with
  `single_lower` in the decoder, an inline `to_lowercase` in `char_event`, and
  `single_upper` in `src/event/key.rs` for the inverse. They already differ
  (Caps Lock alone gives `shifted` in `char_key` and not in `char_event`), and a
  fix to one does not reach the others. One function taking whether Shift was
  held and whether Caps Lock is known would hold the knowledge once.
- **`WindowSize` is defined twice**, in `src/terminal/unix.rs` and
  `src/terminal/windows.rs`, with the same fields and different docs; a field
  added to one breaks the other platform in CI only. One definition in
  `src/terminal.rs`, with the per-platform note on the pixel fields.
- **The reader tests are duplicated between `src/reader/tests.rs` and
  `src/reader/windows/tests.rs`**: most tests share a name and a body and differ
  only in how the terminal is driven (a pty write, an injected record). A fix to
  one has to be made twice. A shared body over a small "type these bytes" helper
  per platform would keep them one.

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

- **Nothing in phase 07 has run on a real Windows console yet.** The Windows
  tests (`src/reader/windows/tests.rs`, `src/terminal/windows/tests.rs`) write
  records into a console the test process allocates, so they exercise the
  console API, not Windows Terminal, conhost's own keyboard handling or ConPTY.
- **Focus records are read only with VT input off**, as crossterm reads them,
  although Microsoft documents `FOCUS_EVENT` as the console's internal. With VT
  input on they are dropped, so focus is reported once, by the terminal under
  mode 1004. Not checked on a real console: whether conhost sends them when the
  window gains and loses focus.
- **A key record with no virtual-key and no scan code is text**
  (`KeyRecord::is_text`), read as a terminal's bytes, in win32-input-mode and in
  the console's records. tmxr's review recalled conhost writing its answers to
  queries (DA1, likely the OSC colours) into the input that way; unverified on a
  real console. A character typed with no key behind it (IME, `SendInput`
  without a virtual key) reads as a legacy key with no release either.
- **An Alt code's release is made up**: the console sends its character only
  with Alt's release, so ttyr reports a press and, at once, a release.
- **`window_size` has no pixels on Windows**: the console does not report them
  (`width` and `height` are 0).
- **A modifier or lock key's own record is dropped** (`ConsoleKeys::key`):
  Shift, Ctrl, Alt, the Windows keys and the locks pressed alone are no event,
  as in crossterm, where kitty reports them with flag 8. A consumer that wants
  them gets a `KeyCode::Modifier` mapping (left and right from the scan code or
  `ENHANCED_KEY`).
- **Ctrl with a non-letter key reads the control code**, not the layout: on a
  layout where `[` is not where VK_OEM_4 sits, Ctrl+that key still reads as
  Ctrl+\[ when the console gives 0x1B. Reading the layout needs `ToUnicodeEx`
  (crossterm's way), Windows-only. Where the console gives no control code at
  all (Ctrl+; and the other OEM keys), `us_oem_char` in `src/event/console.rs`
  names the key by its US character, so on another layout it is the US key at
  that place.
- **win32-input-mode's repeat count (`Rc`) is not read.** Windows Terminal sends
  a held key as more records, which read as repeats.

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
