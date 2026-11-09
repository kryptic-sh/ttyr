# ttyr plan — index

ttyr is kryptic-sh's replacement for crossterm: terminal input, output and raw
mode for Rust, with the full kitty keyboard protocol and a ratatui backend,
shared by tmxr, hjkl and the other house projects.

Phases are numbered in build order. The input half (02, 03) comes first: tmxr
and hjkl are both blocked on it.

| #   | Phase                                                | Covers                                                                    | Status  |
| --- | ---------------------------------------------------- | ------------------------------------------------------------------------- | ------- |
| 00  | [00-index.md](00-index.md)                           | This list                                                                 |         |
| 01  | [01-overview.md](01-overview.md)                     | Why ttyr exists, its consumers, principles, decisions, non-goals          | —       |
| 02  | [02-events-and-decoder.md](02-events-and-decoder.md) | Event types; the decoder for kitty, xterm, legacy, mouse and reports      | planned |
| 03  | [03-unix-reader.md](03-unix-reader.md)               | Reading a Unix tty: poll/read, timeouts, SIGWINCH, async                  | planned |
| 04  | [04-terminal-modes.md](04-terminal-modes.md)         | Raw mode, size, the modes a program turns on, restore from a panic        | planned |
| 05  | [05-output.md](05-output.md)                         | Commands: cursor, clear, styles and colours, queued and flushed           | planned |
| 06  | [06-ratatui-backend.md](06-ratatui-backend.md)       | `ratatui_core::backend::Backend` on ttyr                                  | planned |
| 07  | [07-windows.md](07-windows.md)                       | The Windows console: input records, VT input, raw mode, size              | planned |
| 08  | [08-adoption.md](08-adoption.md)                     | Moving tmxr, hjkl and the rest off crossterm; retiring the crossterm fork | planned |
