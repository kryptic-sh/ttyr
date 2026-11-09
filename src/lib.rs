//! Terminal input, output and raw mode for Rust: kryptic-sh's replacement for
//! crossterm.
//!
//! Implemented so far: [`event`], the event types and a pure [`Decoder`]
//! from the bytes a terminal sends to events. Reading a tty, raw mode, output
//! and the ratatui backend are planned in `docs/plan/` in the repository.
//!
//! [`Decoder`]: event::Decoder

pub mod event;
