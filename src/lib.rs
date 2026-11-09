//! Terminal input, output and raw mode for Rust: kryptic-sh's replacement for
//! crossterm.
//!
//! Implemented so far: [`event`], the event types and a pure [`Decoder`]
//! from the bytes a terminal sends to events; and on Unix `reader`, which
//! reads them from a tty, and `terminal`, its size and raw mode. The terminal's
//! modes, output, the ratatui backend and Windows are planned in `docs/plan/`
//! in the repository.
//!
//! [`Decoder`]: event::Decoder

pub mod event;
#[cfg(unix)]
pub mod reader;
#[cfg(unix)]
pub mod terminal;
#[cfg(all(test, unix))]
mod test_pty;
