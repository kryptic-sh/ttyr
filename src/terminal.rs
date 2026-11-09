//! The terminal itself: the modes a program turns on and off, as
//! [`Command`](crate::command::Command)s; and on Unix its size and raw mode.
//!
//! The size and raw mode are Unix only for now; the Windows console's side is
//! planned (phase 07 in `docs/plan/`), so on Windows they do not exist and a
//! use of them fails to compile.

mod modes;
#[cfg(unix)]
mod unix;

pub use modes::*;
#[cfg(unix)]
pub use unix::{
    WindowSize, disable_raw_mode, enable_raw_mode, is_raw_mode_enabled, size, window_size,
};
#[cfg(unix)]
pub(crate) use unix::{open_tty, window_size_of};
