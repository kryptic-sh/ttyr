# 01 — Overview

## Why

crossterm is the bottleneck for the kryptic-sh terminal apps' more advanced
input (decided by the owner, 2026-11-09, to replace it entirely):

- **It drops what the kitty keyboard protocol reports.** With alternate keys
  (flag 4) it swaps in the shifted character, clears Shift, and discards the
  unshifted key and the base-layout key; it has no flag 16 (associated text),
  its `REPORT_ASSOCIATED_TEXT` commented out. Read in crossterm 0.29's
  `parse_csi_u_encoded_key_code`.
- **It cannot read xterm's modifyOtherKeys form** (`CSI 27 ; mods ; key ~`).
- **Its kitty probe blocks** (`supports_keyboard_enhancement` sends a query and
  waits), which hangs PTY test harnesses; hjkl pushes its flags unprobed for
  that reason.
- **Its reader needed fixing.** tmxr runs a fork, `kryptic-sh/crossterm`, whose
  commits are all reader fixes: Alt+Esc parsed on Unix (crossterm#1143), a
  SIGWINCH that comes with input kept, an unknown `CSI ?` report no longer
  swallowing later input (crossterm#1106), a burst longer than one read
  delivered whole and a read at a time (crossterm#1155), and the terminal's
  colour-scheme and colour reports read.

## Consumers

- **tmxr** (terminal multiplexer). Its plan 18 (`docs/plan/18-kitty-keyboard.md`
  in tmxr) needs every kitty flag, the xterm tier and a legacy fallback; its
  step 3 is blocked on ttyr's input. It uses: `event::{poll, read}`, `Event`,
  `KeyEvent`, `KeyCode`, `KeyModifiers`, `MouseEvent`, colour-scheme and colour
  reports (`ColorScheme`, `TerminalColor`, `ColorSlot`), `Event::Resize`, raw
  mode, `terminal::size`, `execute!`, `cursor::Show`, keyboard-flag push/pop,
  and crossterm's `serde` feature: tmxr sends `Event`s between its client and
  server (`ClientMsg::Input` in `tmxr-proto`). It writes its own screen bytes;
  it does not draw with ratatui.
- **hjkl** (vim engine and editor; its plan `docs/plan/01-kitty-keyboard.md`,
  phase 3, points at ttyr). Its requirements, as its session read its tree on
  2026-11-09:
  - Input: blocking `poll(timeout)` + `read()` only; `EventStream` is unused
    (four crates enable `event-stream` without using it), so async is optional
    for hjkl.
  - Events: keys with press, repeat and release; mouse down, up, drag, moved and
    scroll in four directions, each with button and modifiers; `Resize(w, h)`;
    `Paste(String)`; focus gained and lost.
  - Keys: `Char`, Enter, Esc, Tab, BackTab, Backspace, Delete, Insert, arrows,
    Home, End, PageUp, PageDown, `F(n)`; Shift, Ctrl, Alt and whatever kitty
    adds; key state (as crossterm's `KeyEventState`); a cheap public constructor
    like `KeyEvent::new(code, mods)`, since tests, macro replay and keymap
    replay build events by hand.
  - Terminal: raw mode, alternate screen, mouse capture, bracketed paste and
    focus reporting on and off; keyboard flags push, pop and set; `size()`,
    `Clear`, `SetCursorStyle`; all of it restorable from a panic hook.
  - Drawing: a ratatui backend replacing `CrosstermBackend`; hjkl issues no
    cursor or colour commands of its own.
  - Must not: a blocking support probe on the startup path. Support comes in
    through the event stream (the `CSI ? u` and DA1 replies).
  - hjkl-keymap-tui, hjkl-engine-tui, hjkl-picker-tui (used by tmxr) and
    hjkl-kitty (used by gpur) expose crossterm types publicly; moving them to
    ttyr's is a breaking bump, coordinated across repos.
- **gpur, pikr, sqeel, hodl and the other ratatui apps**: not surveyed yet. Each
  gets surveyed before phase 08 moves it.

## Principles

- **Lossless input.** Every field a terminal reports reaches the caller: the
  kitty key, shifted key, base-layout key, every modifier and lock state, the
  event type, the associated text. Nothing folds Shift into a character or out
  of it on the caller's behalf.
- **Never block on the terminal.** Capability queries are written, and their
  replies arrive as events like any other. A caller that wants to wait for one
  does so with its own timeout.
- **Restore always.** Every mode ttyr turns on, it can turn off from a panic
  hook or a signal path, in the reverse order, idempotently.
- **The general form.** Unix and Windows are both implemented; a missing side
  fails loudly, never as a silent no-op.
- **Tested against real bytes.** Decoder tests use sequences captured from real
  terminals and the kitty spec's own examples, named by source.

## Decisions

- Name `ttyr`, in the kryptic-sh org (owner, 2026-11-09).
- A full replacement, output and a ratatui backend included (owner, 2026-11-09).
- MIT, edition 2024, MSRV 1.95, the org's shared clippy lint set, as vt1k.
- **API shape: crossterm's names where the meaning is identical, new names where
  it moves** (agreed with hjkl's session, 2026-11-09). A field that keeps its
  name but changes meaning is the trap: crossterm's `KeyEvent.code` under kitty
  flag 4 is the shifted character with Shift cleared, kitty's key is the
  unshifted one, so a ttyr `code` would leave every ported
  `match code { Char('A') => … }` compiling and silently not matching.
  - Same names, same meaning: `Event` and its `Key`, `Mouse`, `Resize`, `Paste`,
    `FocusGained`, `FocusLost`; `KeyCode`'s variants;
    `KeyModifiers::{SHIFT, CONTROL, ALT}` (bitflags, crossterm's const names);
    `KeyEventKind`; `MouseEvent`, `MouseEventKind`, `MouseButton`; `poll`,
    `read`, `size`; `execute!`, `queue!`; `Clear`, `SetCursorStyle`,
    `EnterAlternateScreen` and the other commands.
  - New names where the meaning moves: `KeyEvent` has no `code`; it has `key`
    (the unshifted key the terminal reported), `shifted`, `base_layout` and
    `text` (each an `Option`), so a port fails to compile at every `.code` and
    each site chooses the raw key or 02's `logical_char()` on purpose.
  - `KeyEvent` and `Event` are `#[non_exhaustive]`, built with
    `KeyEvent::new(key, modifiers)` and builder methods for the optional fields,
    so a later field is not a break; `KeyEvent { key, modifiers, .. }` still
    matches.
  - Super, Hyper and Meta are in `KeyModifiers`; Caps Lock and Num Lock are in
    `KeyEventState` beside `KEYPAD`, never in the modifiers, so a keymap's
    `modifiers == CONTROL` holds with Caps Lock on (agreed with hjkl's session,
    2026-11-09).
  - `KeyEvent` is `Clone` but not `Copy`: kitty's associated text is a `String`.
    hjkl checked its tree and does not need `Copy` (2026-11-09).
  - The decoder is public (`ttyr::event::Decoder`, bytes in, events out): tmxr
    feeds it captured bytes in tests and reads its attach probe's answers with
    it. A flush (`Decoder::flush_event`) is the caller's, so "no escape timeout"
    works: tmxr's client sends a lone Escape at once and applies `escape-time`
    in its server (tmxr's session, 2026-11-09).
  - `serde` is a plain derive: tmxr versions its own protocol and builds both
    ends from one tree, so ttyr promises no stable wire shape.
- **The restore guard** (04, reviewed by tmxr's session, 2026-11-10):
  - It's free functions over one record per process (`ttyr::terminal::enable`,
    `disable`, `restore`), not an RAII value: both apps restore on several
    paths, and the panic hook has to reach the record. tmxr keeps its own thin
    guard whose `Drop` calls `restore`.
  - One of each kind of mode is on at a time. Enabling another value (a
    different mouse level, different kitty flags) turns the old one off and the
    new one on in its place.
  - Mouse capture comes in levels (`MouseTracking`: 1000, 1002 or 1003, with or
    without 1006), so tmxr can follow a pane's own mode. `EnableMouseCapture`
    stays as crossterm's everything form.
  - `suspend_with(out, f)` gives the terminal back around `f`: tmxr's lock
    command, hjkl's `:!cmd`. `suspend` is built on it. On the way back, the
    modes return with their latest values, and raw mode reads the terminal's
    attributes afresh, so what the program in between changed is kept.
  - `restore_on_panic` installs its hook once and calls the previous hook after
    restoring. It writes to the tty raw mode is on (or `/dev/tty`), not stdout,
    whose lock the panicking thread may hold.
  - The colour scheme query (`RequestColorScheme`) is a command of its own, so
    turning the reports back on doesn't ask again.
  - On Windows (07), `enable` and `disable` also set the console's modes behind
    the same signatures.
- **Reading a Unix tty** (03, settled with tmxr's session, 2026-11-10):
  - The system calls go through `rustix` and `SIGWINCH` through `signal-hook`
    (the owner's choice over hand-written libc FFI; crossterm uses both, so
    neither is new to tmxr's or hjkl's tree).
  - `ttyr::reader::Reader` waits with `select`, not `poll`: macOS's `poll` does
    not support devices, ttys among them.
  - The escape timeout defaults to none, as crossterm behaves, and is set with
    `Reader::set_escape_timeout`.
  - `SIGWINCH` is watched before `Reader::open` returns, so tmxr compares the
    size once with the one its hello sent and misses no resize.
  - One reader per process, released on drop so tmxr can re-attach; it reads
    only inside `poll` and `read`, never on a thread of its own, so a paused
    tmxr client leaves the tty to its lock command.
  - A `Waker` (`Send`, `Sync`, `Clone`) ends a wait with `Event::Wake`; it
    replaces tmxr's `raise(SIGWINCH)` to break crossterm's read.

## Open questions

- **Async.** Which runtime-neutral shape (a `Stream` over a non-blocking fd) and
  behind which feature. Neither tmxr nor hjkl needs it (both read on a blocking
  thread; tmxr has no tokio), so 03 shipped without it.
- **hjkl-kitty**: settled with hjkl's session (2026-11-09). Its push/pop moves
  into 04 and its normalising into `KeyEvent::normalize_legacy` (02, done),
  after which it is deleted.

## Non-goals

- Terminal emulation (that is vt1k) and widgets (that is ratatui).
- Terminfo-driven output: ttyr writes the ANSI/xterm sequences modern terminals
  take, as crossterm does.
