# 08 — Adoption

In the order the work unblocks:

1. **tmxr**, input first (its plan 18 step 3): the client reads with ttyr, the
   protocol carries ttyr's key type, and its crossterm fork patch
   (`[patch.crates-io]` in tmxr's `Cargo.toml`) goes once nothing in tmxr uses
   crossterm. tmxr ships on Windows too, so its Unix client can move after 02
   and 03 but crossterm leaves tmxr only after 07.
2. **hjkl**, once 02 to 07 have landed: hjkl ships `x86_64-pc-windows-msvc`,
   runs its tests on windows-latest and relies on the Windows console's release
   for every key-up (`apps/hjkl/src/app/tests/key_event_kind.rs` in hjkl), so 07
   is a prerequisite, as are 04 and 06. hjkl-keymap-tui, hjkl-engine-tui,
   hjkl-picker-tui and hjkl-kitty switch their public types to ttyr's, a
   breaking bump released together, with tmxr (which calls
   `hjkl-picker-tui::handle_key`) told before it ships. hjkl-kitty goes away
   once 02's helpers replace its normalising and 04's guard its flag push and
   pop.
3. **gpur** (uses hjkl-kitty), then the other ratatui apps, each surveyed first.
4. The `kryptic-sh/crossterm` fork is archived when nothing depends on it.

Each port is its own change in its own repo, tested there; ttyr releases the
version it needs first.
