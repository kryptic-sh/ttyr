//! Terminal input, output and raw mode for Rust: kryptic-sh's replacement for
//! crossterm.
//!
//! Implemented so far: [`event`], the event types and a pure [`Decoder`]
//! from the bytes a terminal sends to events; [`reader`], which reads them
//! from a Unix tty or the Windows console; [`command`], crossterm's `Command`
//! with `queue!` and `execute!`; the commands themselves in [`cursor`],
//! [`style`] and [`terminal`], which also has the capability queries, the
//! terminal's size and raw mode, and the guard that restores it. The ratatui
//! backend is planned in `docs/plan/` in the repository.
//!
//! [`Decoder`]: event::Decoder

pub mod command;
pub mod cursor;
pub mod event;
pub mod reader;
pub mod style;
pub mod terminal;
#[cfg(test)]
mod test_bytes;
#[cfg(all(test, windows))]
mod test_console;
#[cfg(all(test, unix))]
mod test_pty;
