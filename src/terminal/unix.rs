//! The Unix terminal's size and raw mode.

use std::fs::File;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rustix::fs::{Mode, OFlags};
use rustix::termios::{OptionalActions, Termios, tcgetattr, tcgetwinsize, tcsetattr};

use super::WindowSize;

#[cfg(test)]
mod tests;

/// The controlling terminal's size, columns then rows; with no controlling
/// terminal, stdout's (see [`window_size`]).
pub fn size() -> io::Result<(u16, u16)> {
    let size = window_size()?;
    Ok((size.columns, size.rows))
}

/// The controlling terminal's size, in cells and pixels. A program with no
/// controlling terminal (a `setsid` launcher, some service managers) gets
/// stdout's instead, as crossterm measures, and the error measuring it when
/// stdout is not a terminal either.
pub fn window_size() -> io::Result<WindowSize> {
    window_size_with(open_tty, io::stdout())
}

/// [`window_size`] of the terminal `open` opens, or of `fallback` when it
/// cannot be opened.
fn window_size_with(
    open: impl FnOnce() -> io::Result<OwnedFd>,
    fallback: impl AsFd,
) -> io::Result<WindowSize> {
    match open() {
        Ok(tty) => window_size_of(tty),
        Err(_) => window_size_of(fallback),
    }
}

/// The size of the terminal `tty`, columns then rows.
pub fn size_of(tty: impl AsFd) -> io::Result<(u16, u16)> {
    let size = window_size_of(tty)?;
    Ok((size.columns, size.rows))
}

/// The size of the terminal `tty`, in cells and pixels.
pub fn window_size_of(tty: impl AsFd) -> io::Result<WindowSize> {
    let size = tcgetwinsize(tty)?;
    Ok(WindowSize {
        columns: size.ws_col,
        rows: size.ws_row,
        width: size.ws_xpixel,
        height: size.ws_ypixel,
    })
}

/// The controlling terminal, `/dev/tty`, whatever stdin and stdout are.
pub fn open_tty() -> io::Result<OwnedFd> {
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
    enable_raw_mode_with(open_tty)
}

/// [`enable_raw_mode`] on `tty`, a terminal that need not be the controlling
/// one: stdin, say, for a program whose session has no controlling terminal.
/// A duplicate of `tty` is kept, so [`disable_raw_mode`], the guard's
/// `restore` and its panic hook put back the attributes of this terminal.
/// Calling it while raw mode is on (on any terminal) does nothing.
pub fn enable_raw_mode_on(tty: impl AsFd) -> io::Result<()> {
    enable_raw_mode_with(|| tty.as_fd().try_clone_to_owned())
}

/// [`enable_raw_mode`] on the terminal `open` opens, opened only if raw mode
/// is off.
pub fn enable_raw_mode_with(open: impl FnOnce() -> io::Result<OwnedFd>) -> io::Result<()> {
    let mut raw = raw();
    if raw.is_none() {
        *raw = Some(make_raw(open()?)?);
    }
    Ok(())
}

/// Puts back the attributes the terminal had before [`enable_raw_mode`].
/// Calling it while raw mode is off does nothing.
pub fn disable_raw_mode() -> io::Result<()> {
    take_raw().map(drop)
}

/// [`disable_raw_mode`], returning the terminal raw mode was on so that
/// [`enable_raw_mode_with`] can put it back in raw mode; `None` when it was off.
pub fn take_raw() -> io::Result<Option<OwnedFd>> {
    let mut raw = raw();
    if let Some((tty, original)) = raw.as_ref() {
        tcsetattr(tty, OptionalActions::Now, original)?;
    }
    Ok(raw.take().map(|(tty, _)| tty))
}

/// [`take_raw`] for a panic hook, which may run while this thread holds the
/// state: it is skipped then, and a failure has nowhere to be reported.
pub fn take_raw_after_panic() -> Option<OwnedFd> {
    let (tty, original) = super::lock_after_panic(&RAW)?.take()?;
    // A panic hook has nowhere to report a failure; the attributes stay raw.
    let _ = tcsetattr(&tty, OptionalActions::Now, &original);
    Some(tty)
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

/// What raw mode changes: the terminal, to put back.
pub type RawTty = OwnedFd;

/// Where a panic hook writes the modes' off sequences: the terminal raw mode
/// was on, duplicated so that it is kept to resume on, or else `/dev/tty`.
pub fn panic_output(raw: Option<&OwnedFd>) -> io::Result<File> {
    Ok(File::from(match raw {
        Some(tty) => tty.try_clone()?,
        None => open_tty()?,
    }))
}
