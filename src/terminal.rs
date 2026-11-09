//! The terminal itself: the modes a program turns on and off, and the
//! queries it asks, as [`Command`](crate::command::Command)s; and on Unix its size, raw mode, and
//! the guard that turns every mode off again (`enable`, `restore`).
//!
//! The size, raw mode and the guard are Unix only for now; the Windows console's side is
//! planned (phase 07 in `docs/plan/`), so on Windows they do not exist and a
//! use of them fails to compile.

#[cfg(unix)]
use std::sync::{Mutex, MutexGuard, TryLockError};

#[cfg(unix)]
mod guard;
mod modes;
mod queries;
#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use guard::{Mode, disable, enable, restore, restore_on_panic, suspend, suspend_with};
pub use modes::*;
pub use queries::*;
#[cfg(unix)]
pub use unix::{
    WindowSize, disable_raw_mode, enable_raw_mode, is_raw_mode_enabled, size, window_size,
};
#[cfg(unix)]
pub(crate) use unix::{open_tty, window_size_of};

/// Locks `state` from a panic hook, which may run while this thread holds it
/// (the panic came from inside): `None` then, rather than a deadlock. A
/// poisoned lock is taken as it is.
#[cfg(unix)]
fn lock_after_panic<T>(state: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match state.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}
