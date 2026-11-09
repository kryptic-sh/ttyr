//! Terminal input, output and raw mode for Rust: kryptic-sh's replacement for
//! crossterm.
//!
//! Implemented so far: [`event`], the event types and a pure [`Decoder`]
//! from the bytes a terminal sends to events; [`command`], crossterm's
//! `Command` with `queue!` and `execute!`; the commands themselves in
//! [`cursor`], [`style`] and [`terminal`], which also has the capability
//! queries and, on Unix, the terminal's size, raw mode and the guard that
//! restores it; and on Unix `reader`, which reads events from a tty. The
//! ratatui backend and Windows are planned in `docs/plan/` in the repository.
//!
//! [`Decoder`]: event::Decoder

pub mod command;
pub mod cursor;
pub mod event;
#[cfg(unix)]
pub mod reader;
pub mod style;
pub mod terminal;
#[cfg(test)]
mod test_bytes;
#[cfg(all(test, unix))]
mod test_pty;
