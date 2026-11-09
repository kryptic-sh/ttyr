# 04 — Terminal modes

**In progress.** Landed: raw mode and size (`ttyr::terminal`: `size`,
`window_size`, `enable_raw_mode`, `disable_raw_mode`, `is_raw_mode_enabled`,
crossterm's names; Unix; Windows in 07); each mode's on and off as a command,
with 05's `Command` trait and `queue!`/`execute!` brought forward for them.
Still to do:

- **Modes a program turns on**, each with its off: the alternate screen, mouse
  capture (1000/1002/1003 with 1006, optional 1016), bracketed paste, focus
  reporting, kitty keyboard flags (push `CSI > f u`, pop `CSI < n u`, set
  `CSI = f ; mode u`), xterm modifyOtherKeys (`CSI > 4 ; 2 m` / `CSI > 4 m`),
  the colour-scheme reports (2031), synchronized output (2026).
- **Capability queries without blocking**: helpers that write the query (kitty
  `CSI ? u` followed by DA1, the text area's pixel size `CSI 14 t`, DECRQM,
  XTVERSION, colour queries); the replies come back through 02's report events,
  and a small helper can collect them with the caller's timeout.
- **Modes switch at any time**, not only at startup: hjkl's `:set mouse` turns
  mouse capture on and off while running, and tmxr's client changes kitty flags
  as focus moves (its plan 18). The guard tracks each mode's current state.
- **A guard** that records what is on now and turns it off in reverse on drop,
  from a panic hook, and before a suspend (`SIGTSTP`) with re-entry after
  `SIGCONT`. Idempotent: running it twice does nothing the second time.
- **Tests**: on a pty, each mode's bytes on and off; the guard's restore after a
  panic in a child process.
