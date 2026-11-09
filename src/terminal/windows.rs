//! The Windows console's size, raw mode and input modes, through the console
//! API.

use std::fs::File;
use std::io;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::sync::{Mutex, MutexGuard, PoisonError};

use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    CONSOLE_MODE, CONSOLE_SCREEN_BUFFER_INFO, ENABLE_ECHO_INPUT, ENABLE_EXTENDED_FLAGS,
    ENABLE_LINE_INPUT, ENABLE_MOUSE_INPUT, ENABLE_PROCESSED_INPUT, ENABLE_QUICK_EDIT_MODE,
    ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, ENABLE_WINDOW_INPUT,
    GetConsoleMode, GetConsoleScreenBufferInfo, SetConsoleMode,
};

#[cfg(test)]
mod tests;

/// The terminal's size in cells and, where it reports them, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowSize {
    /// Columns.
    pub columns: u16,
    /// Rows.
    pub rows: u16,
    /// The width in pixels: 0, as the console does not report it.
    pub width: u16,
    /// The height in pixels: 0, as the console does not report it.
    pub height: u16,
}

/// The console's input and its active screen buffer, which raw mode and the
/// guard change the modes of.
#[derive(Debug)]
pub struct Console {
    /// `CONIN$`.
    pub input: OwnedHandle,
    /// `CONOUT$`.
    pub output: OwnedHandle,
}

impl Console {
    /// The process's console, whatever its standard handles are.
    pub fn open() -> io::Result<Self> {
        Ok(Self {
            input: open_console("CONIN$")?,
            output: open_console("CONOUT$")?,
        })
    }
}

/// What raw mode changes is kept as on Unix: the terminal, to put back.
pub type RawTty = Console;

/// Opens `CONIN$` or `CONOUT$`, read and write, so that its mode can be set.
pub fn open_console(name: &str) -> io::Result<OwnedHandle> {
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the
    // call; the security attributes and template are null, as allowed.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a handle CreateFileW just returned is valid, and nothing else
    // owns it.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

/// `handle` as the console API takes it.
fn raw(handle: BorrowedHandle<'_>) -> HANDLE {
    handle.as_raw_handle()
}

/// A console handle's mode.
pub fn console_mode(handle: BorrowedHandle<'_>) -> io::Result<CONSOLE_MODE> {
    let mut mode = 0;
    // SAFETY: the handle is borrowed, so open; `mode` is a valid out
    // pointer for the call.
    if unsafe { GetConsoleMode(raw(handle), &mut mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(mode)
}

/// Sets a console handle's mode.
pub fn set_console_mode(handle: BorrowedHandle<'_>, mode: CONSOLE_MODE) -> io::Result<()> {
    // SAFETY: the handle is borrowed, so open.
    if unsafe { SetConsoleMode(raw(handle), mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Turns `on` bits on and `off` bits off in the console input's mode.
pub fn change_input_mode(on: CONSOLE_MODE, off: CONSOLE_MODE) -> io::Result<()> {
    let input = open_console("CONIN$")?;
    let mode = console_mode(input.as_handle())?;
    set_console_mode(input.as_handle(), (mode | on) & !off)
}

/// Mouse records from the console (`ENABLE_MOUSE_INPUT`), which takes Quick
/// Edit's selection off the mouse; off, Quick Edit is not put back until
/// raw mode is turned off.
pub fn set_mouse_input(on: bool) -> io::Result<()> {
    if on {
        change_input_mode(
            ENABLE_MOUSE_INPUT | ENABLE_EXTENDED_FLAGS,
            ENABLE_QUICK_EDIT_MODE,
        )
    } else {
        change_input_mode(0, ENABLE_MOUSE_INPUT)
    }
}

/// VT input (`ENABLE_VIRTUAL_TERMINAL_INPUT`): the console hands over the
/// bytes a terminal sends, which the decoder reads, in place of its records.
pub fn set_virtual_terminal_input(on: bool) -> io::Result<()> {
    if on {
        change_input_mode(ENABLE_VIRTUAL_TERMINAL_INPUT, 0)
    } else {
        change_input_mode(0, ENABLE_VIRTUAL_TERMINAL_INPUT)
    }
}

/// The console's size, columns then rows.
pub fn size() -> io::Result<(u16, u16)> {
    let size = window_size()?;
    Ok((size.columns, size.rows))
}

/// The console's size in cells. The console does not say its pixels.
pub fn window_size() -> io::Result<WindowSize> {
    window_size_of(open_console("CONOUT$")?.as_handle())
}

/// The size of the window onto the screen buffer `output`.
pub fn window_size_of(output: BorrowedHandle<'_>) -> io::Result<WindowSize> {
    let window = screen_buffer_info(output)?.srWindow;
    let cells = |low: i16, high: i16| u16::try_from(i32::from(high) - i32::from(low) + 1);
    Ok(WindowSize {
        columns: cells(window.Left, window.Right).map_err(io::Error::other)?,
        rows: cells(window.Top, window.Bottom).map_err(io::Error::other)?,
        width: 0,
        height: 0,
    })
}

/// The screen buffer row at the window's top, which mouse records count
/// from.
pub fn window_top(output: BorrowedHandle<'_>) -> io::Result<i16> {
    Ok(screen_buffer_info(output)?.srWindow.Top)
}

fn screen_buffer_info(output: BorrowedHandle<'_>) -> io::Result<CONSOLE_SCREEN_BUFFER_INFO> {
    // SAFETY: CONSOLE_SCREEN_BUFFER_INFO is plain data; zero is a valid
    // value of every field.
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: the handle is borrowed, so open; `info` is a valid out
    // pointer for the call.
    if unsafe { GetConsoleScreenBufferInfo(raw(output), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

/// While raw mode is on: the console, and its input and output modes from
/// before, to put back.
static RAW: Mutex<Option<(Console, CONSOLE_MODE, CONSOLE_MODE)>> = Mutex::new(None);

fn raw_state() -> MutexGuard<'static, Option<(Console, CONSOLE_MODE, CONSOLE_MODE)>> {
    RAW.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Puts the console in raw mode: input a key at a time, no echo, no Ctrl+C
/// handling, resize records on; and VT processing on its output, so the
/// commands' sequences work. Its modes from before are kept for
/// [`disable_raw_mode`]. Calling it while raw mode is on does nothing.
pub fn enable_raw_mode() -> io::Result<()> {
    enable_raw_mode_with(Console::open)
}

/// [`enable_raw_mode`] on the console `open` opens, opened only if raw mode
/// is off.
pub fn enable_raw_mode_with(open: impl FnOnce() -> io::Result<Console>) -> io::Result<()> {
    let mut raw = raw_state();
    if raw.is_none() {
        let console = open()?;
        let input = console_mode(console.input.as_handle())?;
        let output = console_mode(console.output.as_handle())?;
        let raw_input = (input | ENABLE_WINDOW_INPUT)
            & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT);
        set_console_mode(console.input.as_handle(), raw_input)?;
        if let Err(err) = set_console_mode(
            console.output.as_handle(),
            output | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        ) {
            set_console_mode(console.input.as_handle(), input)?;
            return Err(err);
        }
        *raw = Some((console, input, output));
    }
    Ok(())
}

/// Puts back the modes the console had before [`enable_raw_mode`]. Calling
/// it while raw mode is off does nothing.
pub fn disable_raw_mode() -> io::Result<()> {
    take_raw().map(drop)
}

/// [`disable_raw_mode`], returning the console so that
/// [`enable_raw_mode_with`] can put it back in raw mode; `None` when raw mode
/// was off.
pub fn take_raw() -> io::Result<Option<Console>> {
    let mut raw = raw_state();
    if let Some((console, input, output)) = raw.as_ref() {
        set_console_mode(console.input.as_handle(), *input)?;
        set_console_mode(console.output.as_handle(), *output)?;
    }
    Ok(raw.take().map(|(console, _, _)| console))
}

/// [`take_raw`] for a panic hook, which may run while this thread holds the
/// state: it is skipped then, and a failure has nowhere to be reported.
pub fn take_raw_after_panic() -> Option<Console> {
    let (console, input, output) = super::lock_after_panic(&RAW)?.take()?;
    // A panic hook has nowhere to report a failure; the modes stay raw.
    let _ = set_console_mode(console.input.as_handle(), input);
    let _ = set_console_mode(console.output.as_handle(), output);
    Some(console)
}

/// Whether [`enable_raw_mode`] has put the console in raw mode.
pub fn is_raw_mode_enabled() -> bool {
    raw_state().is_some()
}

/// Where a panic hook writes the modes' off sequences: the console's
/// output.
pub fn panic_output(raw: Option<Console>) -> io::Result<File> {
    let output = match raw {
        Some(console) => console.output,
        None => open_console("CONOUT$")?,
    };
    Ok(File::from(output))
}
