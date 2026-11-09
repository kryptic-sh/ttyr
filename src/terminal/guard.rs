//! The restore guard: which modes a program has turned on, so that every one
//! is turned off again, in reverse, on the way out, from a panic hook, and
//! around a suspend.
//!
//! The record is per process, as the terminal is, so a panic hook can reach
//! it. Modes written with the commands directly (`execute!(out,
//! EnableMouseCapture)`) are not in it.
//!
//! On Windows `enable` and `disable` also set the console's input modes the
//! mode needs (mouse input, VT input).

use std::io::{self, Write};
use std::mem;
use std::sync::{Mutex, MutexGuard, Once, PoisonError};

use super::modes::{
    DisableBracketedPaste, DisableColorSchemeReports, DisableFocusChange, DisableModifyOtherKeys,
    DisableMouseCapture, EnableBracketedPaste, EnableColorSchemeReports, EnableFocusChange,
    EnableModifyOtherKeys, EnterAlternateScreen, LeaveAlternateScreen, MouseTracking,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags, SetMouseTracking,
};
use super::modes::{DisableWin32InputMode, EnableWin32InputMode};
use super::sys::{
    RawTty, enable_raw_mode_with, is_raw_mode_enabled, panic_output, take_raw, take_raw_after_panic,
};
use crate::command::Command;
use crate::cursor::Show;
use crate::event::KeyboardEnhancementFlags;

#[cfg(all(test, unix))]
mod tests;

/// The modes on, in the order they were turned on.
static ON: Mutex<Vec<Mode>> = Mutex::new(Vec::new());

fn on() -> MutexGuard<'static, Vec<Mode>> {
    ON.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A mode the guard turns on and off. One of each kind is on at a time:
/// enabling a kind that is on with another value replaces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Mode {
    /// The alternate screen.
    AlternateScreen,
    /// Mouse reports, at one level (see [`SetMouseTracking`]).
    Mouse {
        /// Which events.
        tracking: MouseTracking,
        /// Whether in SGR's form (1006).
        sgr: bool,
    },
    /// Bracketed paste.
    BracketedPaste,
    /// Focus reports.
    FocusChange,
    /// The colour scheme reports (2031); the query for the scheme now is
    /// [`RequestColorScheme`](super::RequestColorScheme), written apart.
    ColorSchemeReports,
    /// xterm's modifyOtherKeys, level 2.
    ModifyOtherKeys,
    /// Kitty keyboard flags, pushed on the terminal's stack and popped when
    /// turned off.
    KeyboardFlags(KeyboardEnhancementFlags),
    /// Windows Terminal's win32-input-mode (`CSI ? 9001 h`): every key as a
    /// console key record in VT input, read as `KeyEncoding::Windows`, with
    /// releases. Other terminals ignore it.
    Win32Input,
    /// The Windows console's VT input (`ENABLE_VIRTUAL_TERMINAL_INPUT`): the
    /// reader gets the bytes the terminal sends, kitty's and xterm's
    /// sequences among them, in place of the console's own key records.
    #[cfg(windows)]
    VirtualTerminalInput,
}

impl Mode {
    /// Whether `other` is the same mode, whatever its value.
    fn same_kind(&self, other: &Self) -> bool {
        mem::discriminant(self) == mem::discriminant(other)
    }

    /// Writes the mode's on sequence.
    fn write_on(&self, bytes: &mut String) -> io::Result<()> {
        match *self {
            Self::AlternateScreen => push(bytes, EnterAlternateScreen),
            Self::Mouse { tracking, sgr } => push(bytes, SetMouseTracking { tracking, sgr }),
            Self::BracketedPaste => push(bytes, EnableBracketedPaste),
            Self::FocusChange => push(bytes, EnableFocusChange),
            Self::ColorSchemeReports => push(bytes, EnableColorSchemeReports),
            Self::ModifyOtherKeys => push(bytes, EnableModifyOtherKeys),
            Self::KeyboardFlags(flags) => push(bytes, PushKeyboardEnhancementFlags(flags)),
            Self::Win32Input => push(bytes, EnableWin32InputMode),
            #[cfg(windows)]
            Self::VirtualTerminalInput => Ok(()),
        }
    }

    /// Writes the mode's off sequence.
    fn write_off(&self, bytes: &mut String) -> io::Result<()> {
        match self {
            Self::AlternateScreen => push(bytes, LeaveAlternateScreen),
            Self::Mouse { .. } => push(bytes, DisableMouseCapture),
            Self::BracketedPaste => push(bytes, DisableBracketedPaste),
            Self::FocusChange => push(bytes, DisableFocusChange),
            Self::ColorSchemeReports => push(bytes, DisableColorSchemeReports),
            Self::ModifyOtherKeys => push(bytes, DisableModifyOtherKeys),
            Self::KeyboardFlags(_) => push(bytes, PopKeyboardEnhancementFlags),
            Self::Win32Input => push(bytes, DisableWin32InputMode),
            #[cfg(windows)]
            Self::VirtualTerminalInput => Ok(()),
        }
    }

    /// Sets the console input modes the mode needs besides its sequence:
    /// mouse records for the mouse, and VT input.
    #[cfg(windows)]
    fn set_console(&self, on: bool) -> io::Result<()> {
        match self {
            Self::Mouse { .. } => super::windows::set_mouse_input(on),
            Self::VirtualTerminalInput => super::windows::set_virtual_terminal_input(on),
            _ => Ok(()),
        }
    }
}

/// Appends `command`'s bytes.
fn push(bytes: &mut String, command: impl Command) -> io::Result<()> {
    command
        .write_ansi(bytes)
        .map_err(|_| io::Error::other("a command failed to format"))
}

/// The off sequences of `modes`, in reverse, and the cursor shown.
fn offs(modes: &[Mode]) -> io::Result<String> {
    let mut bytes = String::new();
    for mode in modes.iter().rev() {
        mode.write_off(&mut bytes)?;
    }
    // A program that drew may have hidden it.
    push(&mut bytes, Show)?;
    Ok(bytes)
}

/// Turns `mode` on, queuing its sequence into `out` (the caller flushes),
/// and records it. Already on with the same value, it does nothing; on with
/// another value (another mouse level, other keyboard flags), the old is
/// turned off and the new on in its place.
pub fn enable(out: &mut impl Write, mode: Mode) -> io::Result<()> {
    let mut on = on();
    let mut bytes = String::new();
    let at = on.iter().position(|m| m.same_kind(&mode));
    if let Some(i) = at {
        if on[i] == mode {
            return Ok(());
        }
        on[i].write_off(&mut bytes)?;
    }
    mode.write_on(&mut bytes)?;
    out.write_all(bytes.as_bytes())?;
    #[cfg(windows)]
    mode.set_console(true)?;
    match at {
        Some(i) => on[i] = mode,
        None => on.push(mode),
    }
    Ok(())
}

/// Turns off the mode of `mode`'s kind that is on, whatever its value,
/// queuing its sequence into `out`. Off already, it does nothing.
pub fn disable(out: &mut impl Write, mode: Mode) -> io::Result<()> {
    let mut on = on();
    let Some(i) = on.iter().position(|m| m.same_kind(&mode)) else {
        return Ok(());
    };
    let mut bytes = String::new();
    on[i].write_off(&mut bytes)?;
    out.write_all(bytes.as_bytes())?;
    #[cfg(windows)]
    on[i].set_console(false)?;
    on.remove(i);
    Ok(())
}

/// Turns every recorded mode off in reverse, shows the cursor, flushes
/// `out`, then turns raw mode off. With nothing on and raw mode off, it does
/// nothing. The record is cleared even if writing fails, as a terminal that
/// cannot be written is gone.
pub fn restore(out: &mut impl Write) -> io::Result<()> {
    release(out).map(drop)
}

/// [`restore`], returning the modes that were on and the terminal raw mode
/// was on, to turn back on.
fn release(out: &mut impl Write) -> io::Result<(Vec<Mode>, Option<RawTty>)> {
    let modes = mem::take(&mut *on());
    // Each mode puts back the console's bits it changed, before raw mode
    // puts back the rest.
    #[cfg(windows)]
    for mode in modes.iter().rev() {
        mode.set_console(false)?;
    }
    let wrote = if modes.is_empty() && !is_raw_mode_enabled() {
        Ok(())
    } else {
        offs(&modes).and_then(|bytes| {
            out.write_all(bytes.as_bytes())?;
            out.flush()
        })
    };
    let tty = take_raw();
    wrote?;
    Ok((modes, tty?))
}

/// Gives the terminal back for `f`: restores it (every mode off, raw mode
/// off), runs `f` (a lock program, a shell command), then turns raw mode and
/// every mode back on, with the values they had last. Raw mode takes the
/// terminal's attributes afresh, so what `f` changed in them is kept for when
/// it is next turned off.
pub fn suspend_with<T>(out: &mut impl Write, f: impl FnOnce() -> T) -> io::Result<T> {
    let (modes, tty) = release(out)?;
    let result = f();
    if let Some(tty) = tty {
        enable_raw_mode_with(|| Ok(tty))?;
    }
    for mode in modes {
        enable(out, mode)?;
    }
    out.flush()?;
    Ok(result)
}

/// Stops the process as Ctrl+Z does outside raw mode (`SIGTSTP`), the
/// terminal given back while it is stopped (see [`suspend_with`]), and
/// returns once the shell continues it. Unix only: Windows has no job
/// control to stop a process with.
#[cfg(unix)]
pub fn suspend(out: &mut impl Write) -> io::Result<()> {
    suspend_with(out, || {
        signal_hook::low_level::raise(signal_hook::consts::SIGTSTP)
    })?
}

/// Installs a panic hook that restores the terminal, then runs the hook that
/// was there before. Installed once however often it is called; a hook set
/// after it that calls the one before runs it too.
///
/// It writes to the terminal raw mode is on, or else to `/dev/tty`, not to
/// stdout, whose lock the panicking thread may hold.
pub fn restore_on_panic() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_after_panic();
            previous(info);
        }));
    });
}

/// [`restore`] from a panic hook. The panic may have come from inside the
/// guard, with its state held: that state is skipped rather than waited for.
/// A failure has nowhere to be reported.
fn restore_after_panic() {
    let modes = super::lock_after_panic(&ON).map(|mut on| mem::take(&mut *on));
    let tty = take_raw_after_panic();
    let modes = modes.unwrap_or_default();
    if modes.is_empty() && tty.is_none() {
        return;
    }
    let Ok(mut out) = panic_output(tty) else {
        return;
    };
    if let Ok(bytes) = offs(&modes) {
        let _ = out.write_all(bytes.as_bytes());
    }
}
