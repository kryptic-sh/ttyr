# ttyr

Terminal input, output and raw mode for Rust, with the full kitty keyboard
protocol and a ratatui backend: kryptic-sh's replacement for crossterm, shared
by [tmxr](https://github.com/kryptic-sh/tmxr),
[hjkl](https://github.com/kryptic-sh/hjkl) and the other kryptic-sh terminal
apps.

**Status: in progress.** The event types and the byte decoder (`ttyr::event`)
and reading a Unix tty (`ttyr::reader`) are implemented; raw mode, output, the
ratatui backend and Windows are planned in [docs/plan](docs/plan/00-index.md).

## License

MIT
