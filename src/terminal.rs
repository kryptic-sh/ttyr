//! The terminal itself: its size, and raw mode.
//!
//! Unix only for now; the Windows console's side is planned (phase 07 in
//! `docs/plan/`), so on Windows this module does not exist and a use of it
//! fails to compile.

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rustix::fs::{Mode, OFlags};
use rustix::termios::{OptionalActions, Termios, tcgetattr, tcgetwinsize, tcsetattr};

#[cfg(test)]
mod tests;

/// The terminal's size in cells and, where it reports them, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowSize {
    /// Columns.
    pub columns: u16,
    /// Rows.
    pub rows: u16,
    /// The width in pixels, or 0 where the terminal does not report it.
    pub width: u16,
    /// The height in pixels, or 0 where the terminal does not report it.
    pub height: u16,
}

/// The controlling terminal's size, columns then rows.
pub fn size() -> io::Result<(u16, u16)> {
    let size = window_size()?;
    Ok((size.columns, size.rows))
}

/// The controlling terminal's size, in cells and pixels.
pub fn window_size() -> io::Result<WindowSize> {
    window_size_of(open_tty()?)
}

/// The size of the terminal `tty` is.
pub(crate) fn window_size_of(tty: impl AsFd) -> io::Result<WindowSize> {
    let size = tcgetwinsize(tty)?;
    Ok(WindowSize {
        columns: size.ws_col,
        rows: size.ws_row,
        width: size.ws_xpixel,
        height: size.ws_ypixel,
    })
}

/// The controlling terminal, `/dev/tty`, whatever stdin and stdout are.
pub(crate) fn open_tty() -> io::Result<OwnedFd> {
    Ok(rustix::fs::open(
        "/dev/tty",
        OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    )?)
}

/// While raw mode is on: the terminal it is on, and its attributes from
/// before, to put back.
static RAW: Mutex<Option<(OwnedFd, Termios)>> = Mutex::new(None);

/// The raw mode state; a panic while it was held (a panic hook restoring the
/// terminal among them) leaves it as consistent as before.
fn raw() -> MutexGuard<'static, Option<(OwnedFd, Termios)>> {
    RAW.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Puts the controlling terminal in raw mode: input a byte at a time, no
/// echo, no signals from Ctrl+C and the like, no output processing. Its
/// attributes from before are kept for [`disable_raw_mode`]. Calling it while
/// raw mode is on does nothing.
pub fn enable_raw_mode() -> io::Result<()> {
    enable_raw_mode_on(open_tty)
}

/// [`enable_raw_mode`] on the terminal `open` opens, opened only if raw mode
/// is off.
fn enable_raw_mode_on(open: impl FnOnce() -> io::Result<OwnedFd>) -> io::Result<()> {
    let mut raw = raw();
    if raw.is_none() {
        *raw = Some(make_raw(open()?)?);
    }
    Ok(())
}

/// Puts back the attributes the terminal had before [`enable_raw_mode`].
/// Calling it while raw mode is off does nothing.
pub fn disable_raw_mode() -> io::Result<()> {
    let mut raw = raw();
    if let Some((tty, original)) = raw.as_ref() {
        tcsetattr(tty, OptionalActions::Now, original)?;
        *raw = None;
    }
    Ok(())
}

/// Whether [`enable_raw_mode`] has put the terminal in raw mode.
pub fn is_raw_mode_enabled() -> bool {
    raw().is_some()
}

/// Puts `tty` in raw mode, returning it with its attributes from before.
fn make_raw(tty: OwnedFd) -> io::Result<(OwnedFd, Termios)> {
    let original = tcgetattr(&tty)?;
    let mut termios = original.clone();
    termios.make_raw();
    tcsetattr(&tty, OptionalActions::Now, &termios)?;
    Ok((tty, original))
}
