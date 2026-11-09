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

## Open questions

- **API shape.** Keep crossterm's names where they fit (`Event`, `KeyEvent`,
  `KeyCode`, `poll`, `read`, `execute!`) so ports are mostly a path change, or
  design fresh. Leaning: crossterm's names for the shared surface, new types
  only where crossterm's cannot carry what the protocol reports.
- **Async.** Which runtime-neutral shape (a `Stream` over a non-blocking fd) and
  behind which feature, given hjkl does not need it today.
- **hjkl-kitty.** Whether it survives or its push/pop moves into ttyr (hjkl's
  session's question, open until ttyr's API exists).

## Non-goals

- Terminal emulation (that is vt1k) and widgets (that is ratatui).
- Terminfo-driven output: ttyr writes the ANSI/xterm sequences modern terminals
  take, as crossterm does.
