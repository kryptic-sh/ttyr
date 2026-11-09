//! Terminal input, output and raw mode for Rust: kryptic-sh's replacement for
//! crossterm.
//!
//! Implemented so far: [`event`], the event types and a pure [`Decoder`]
//! from the bytes a terminal sends to events; [`command`], crossterm's
//! `Command` with `queue!` and `execute!`; [`terminal`], the modes a program
//! turns on as commands, and on Unix the terminal's size and raw mode; and on
//! Unix `reader`, which reads events from a tty. The restore guard, output,
//! the ratatui backend and Windows are planned in `docs/plan/` in the
//! repository.
//!
//! [`Decoder`]: event::Decoder

pub mod command;
pub mod event;
#[cfg(unix)]
pub mod reader;
pub mod terminal;
#[cfg(all(test, unix))]
mod test_pty;
