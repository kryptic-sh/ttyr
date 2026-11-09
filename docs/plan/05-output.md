# 05 — Output

**Landed early (with 04):** the `Command` trait, `QueueableCommand`,
`ExecutableCommand`, `queue!` and `execute!` (`ttyr::command`).

What ratatui's backend and the apps need, written as ANSI/xterm sequences:

- **Cursor**: move to, show, hide, save, restore, position query (its reply is a
  02 event), cursor style (DECSCUSR) and colour (OSC 12).
- **Clear**: all, from the cursor down and up, the line, to the line's end and
  start; scroll regions and scrolling up and down.
- **Styles**: foreground, background and underline colour (16, 256, RGB); bold,
  dim, italic, the underline styles (single, double, curly, dotted, dashed),
  blink, reverse, hidden, strikethrough, overline; reset.
- **Other**: title (OSC 0/2), hyperlinks (OSC 8), clipboard (OSC 52),
  synchronized output around a frame.
- **Tests**: each command's bytes, compared with the sequences xterm documents.
