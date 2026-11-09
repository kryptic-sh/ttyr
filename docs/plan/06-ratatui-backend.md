# 06 — The ratatui backend

hjkl and the other apps draw only through ratatui (0.30), today through its
crossterm backend.

- Implement `ratatui_core::backend::Backend` (ratatui-core 0.1, read 2026-11-09:
  `draw`, `append_lines`, `hide_cursor`, `show_cursor`, `get_cursor_position`,
  `set_cursor_position`, `clear`, `clear_region`, `size`, `window_size`,
  `flush`, `scroll_region_up`, `scroll_region_down`) on 05's commands, behind a
  `ratatui` feature that depends on ratatui-core alone.
- Draw the same bytes as `ratatui-crossterm` for the same buffer where its
  output is right, so switching backends changes nothing on screen; test that by
  rendering the same buffers through both.
- Cell diffing stays ratatui's; the backend only writes.
