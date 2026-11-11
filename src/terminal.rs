//! The terminal itself: the modes a program turns on and off, and the
//! queries it asks, as [`Command`](crate::command::Command)s; its size and
//! raw mode; and the guard that turns every mode off again (`enable`,
//! `restore`). On Unix that is the tty's; on Windows, the console's.

use std::sync::{Mutex, MutexGuard, TryLockError};

mod guard;
mod modes;
mod queries;
mod screen;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as sys;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as sys;

#[cfg(unix)]
pub use guard::suspend;
pub use guard::{
    Mode, disable, enable, restore, restore_on_panic, resume_after_panic, suspend_with,
};
pub use modes::*;
pub use queries::*;
pub use screen::*;
pub use sys::{disable_raw_mode, enable_raw_mode, is_raw_mode_enabled, size, window_size};
#[cfg(unix)]
pub(crate) use unix::open_tty;
#[cfg(unix)]
pub use unix::{enable_raw_mode_on, size_of, window_size_of};
#[cfg(windows)]
pub(crate) use windows::{Console, console_mode, window_size_of, window_top};

/// The terminal's size in cells and, where it reports them, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowSize {
    /// Columns.
    pub columns: u16,
    /// Rows.
    pub rows: u16,
    /// The width in pixels, or 0 where the terminal does not report it (on
    /// Windows always: the console does not).
    pub width: u16,
    /// The height in pixels, or 0 where the terminal does not report it (on
    /// Windows always: the console does not).
    pub height: u16,
}

/// Locks `state` from a panic hook, which may run while this thread holds it
/// (the panic came from inside): `None` then, rather than a deadlock. A
/// poisoned lock is taken as it is.
fn lock_after_panic<T>(state: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match state.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}
