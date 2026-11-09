# 08 — Adoption

In the order the work unblocks:

1. **tmxr**, input first (its plan 18 step 3): the client reads with ttyr, the
   protocol carries ttyr's key type, and its crossterm fork patch
   (`[patch.crates-io]` in tmxr's `Cargo.toml`) goes once nothing in tmxr uses
   crossterm.
2. **hjkl**: hjkl-keymap-tui, hjkl-engine-tui, hjkl-picker-tui and hjkl-kitty
   switch their public types to ttyr's, a breaking bump released together, with
   tmxr (which calls `hjkl-picker-tui::handle_key`) told before it ships.
3. **gpur** (uses hjkl-kitty), then the other ratatui apps, each surveyed first.
4. The `kryptic-sh/crossterm` fork is archived when nothing depends on it.

Each port is its own change in its own repo, tested there; ttyr releases the
version it needs first.
