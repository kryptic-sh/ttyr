# 04 — Terminal modes

**In progress.** Landed (Unix; Windows in 07), all in `ttyr::terminal`:

- Raw mode and size, with crossterm's names.
- Each mode's on and off as a command. 05's `Command` trait and
  `queue!`/`execute!` were brought forward for them.
- The guard, as tmxr's session reviewed it (2026-11-10): `enable`, `disable`,
  `restore`, `restore_on_panic`, `suspend_with` and `suspend`.

Still to do:

- **Capability queries without blocking**: helpers that write the query (kitty
  `CSI ? u` followed by DA1, the text area's pixel size `CSI 14 t`, DECRQM,
  XTVERSION, colour queries); the replies come back through 02's report events,
  and a small helper can collect them with the caller's timeout.
  `RequestColorScheme` is there already.
- **Optional SGR-pixels mouse (1016)**: no consumer asks for it yet (tmxr
  doesn't).
