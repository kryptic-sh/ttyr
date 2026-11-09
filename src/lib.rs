//! Terminal input, output and raw mode for Rust: kryptic-sh's replacement for
//! crossterm.
//!
//! Implemented so far: [`event`], the event types and a pure [`Decoder`]
//! from the bytes a terminal sends to events, and on Unix `reader`, which
//! reads them from a tty. Raw mode, output, the ratatui backend and Windows
//! are planned in `docs/plan/` in the repository.
//!
//! [`Decoder`]: event::Decoder

pub mod event;
#[cfg(unix)]
pub mod reader;
