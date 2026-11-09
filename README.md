# ttyr

Terminal input, output and raw mode for Rust, with the full kitty keyboard
protocol and a ratatui backend: kryptic-sh's replacement for crossterm, shared
by [tmxr](https://github.com/kryptic-sh/tmxr),
[hjkl](https://github.com/kryptic-sh/hjkl) and the other kryptic-sh terminal
apps.

**Status: in progress.** Implemented: the event types and the byte decoder
(`ttyr::event`), reading a Unix tty (`ttyr::reader`), the terminal's size, raw
mode, modes, restore guard and capability queries (`ttyr::terminal`, Unix), and
crossterm's `Command` with `queue!` and `execute!` (`ttyr::command`). Output,
the ratatui backend and Windows are planned in
[docs/plan](docs/plan/00-index.md).

## License

MIT
