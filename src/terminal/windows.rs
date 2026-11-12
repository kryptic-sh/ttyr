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
    CONSOLE_FONT_INFOEX, CONSOLE_MODE, CONSOLE_SCREEN_BUFFER_INFO, ENABLE_ECHO_INPUT,
    ENABLE_EXTENDED_FLAGS, ENABLE_LINE_INPUT, ENABLE_MOUSE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_QUICK_EDIT_MODE, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    ENABLE_WINDOW_INPUT, GetConsoleMode, GetConsoleScreenBufferInfo, GetCurrentConsoleFontEx,
    SetConsoleMode,
};

#[cfg(test)]
mod tests;

use super::WindowSize;

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

/// The input mode bits a mode changes, as the console had them before the
/// mode turned them, to put back when it is turned off.
struct Saved(Mutex<Option<CONSOLE_MODE>>);

impl Saved {
    const fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// On: saves the bits in `mask` as they are (once, while on), then
    /// turns `on` bits on and `off` bits off. Off: puts the saved bits back.
    /// Quick Edit and Insert mode change only with `ENABLE_EXTENDED_FLAGS`,
    /// which is added whenever the bits put back or turned have them.
    fn switch(
        &self,
        mask: CONSOLE_MODE,
        on_bits: CONSOLE_MODE,
        off_bits: CONSOLE_MODE,
        on: bool,
    ) -> io::Result<()> {
        let saved = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        Self::apply(saved, mask, on_bits, off_bits, on)
    }

    /// [`Saved::switch`] off, from a panic hook: skipped when the panic
    /// came from inside it, with its state held. A failure has nowhere to be
    /// reported.
    fn put_back_after_panic(&self, mask: CONSOLE_MODE) {
        if let Some(saved) = super::lock_after_panic(&self.0) {
            let _ = Self::apply(saved, mask, 0, 0, false);
        }
    }

    fn apply(
        mut saved: MutexGuard<'_, Option<CONSOLE_MODE>>,
        mask: CONSOLE_MODE,
        on_bits: CONSOLE_MODE,
        off_bits: CONSOLE_MODE,
        on: bool,
    ) -> io::Result<()> {
        // Off, the saved bits are taken first: a console that cannot be
        // opened is gone, and its bits mean nothing to the next one.
        let put_back = if on { None } else { Some(saved.take()) };
        let input = open_console("CONIN$")?;
        let mode = console_mode(input.as_handle())?;
        let new = match put_back {
            None => {
                saved.get_or_insert(mode & mask);
                (mode | on_bits) & !off_bits
            }
            Some(Some(bits)) => (mode & !mask) | bits,
            Some(None) => return Ok(()),
        };
        let extended = if (mask | on_bits | off_bits) & ENABLE_QUICK_EDIT_MODE != 0 {
            ENABLE_EXTENDED_FLAGS
        } else {
            0
        };
        set_console_mode(input.as_handle(), new | extended)
    }
}

/// The mouse's bits from before `set_mouse_input(true)`.
static MOUSE: Saved = Saved::new();
/// The VT input bit from before `set_virtual_terminal_input(true)`.
static VT_INPUT: Saved = Saved::new();

/// The input mode bits the mouse changes.
const MOUSE_BITS: CONSOLE_MODE = ENABLE_MOUSE_INPUT | ENABLE_QUICK_EDIT_MODE;

/// The input mode bits the guard's modes own, which they put back
/// themselves: raw mode's snapshot may hold them as a mode turned on
/// before it left them.
const MODE_BITS: CONSOLE_MODE = MOUSE_BITS | ENABLE_VIRTUAL_TERMINAL_INPUT;

/// The input mode raw mode puts back: its snapshot from before, with the
/// modes' bits as they are now, so that a mode turned on before raw mode
/// and off since stays off. (Quick Edit's bit, which changes only with
/// `ENABLE_EXTENDED_FLAGS`, is the console's own either way.) A console
/// whose mode cannot be read gets the snapshot whole.
fn input_to_put_back(console: &Console, snapshot: CONSOLE_MODE) -> CONSOLE_MODE {
    let now = console_mode(console.input.as_handle()).unwrap_or(snapshot);
    (snapshot & !MODE_BITS) | (now & MODE_BITS)
}

/// Puts back the input mode bits the modes changed, for a panic hook.
pub fn put_back_modes_after_panic() {
    MOUSE.put_back_after_panic(MOUSE_BITS);
    VT_INPUT.put_back_after_panic(ENABLE_VIRTUAL_TERMINAL_INPUT);
}

/// Mouse records from the console (`ENABLE_MOUSE_INPUT`), with Quick Edit's
/// selection taken off the mouse; off, both are as they were before.
pub fn set_mouse_input(on: bool) -> io::Result<()> {
    MOUSE.switch(MOUSE_BITS, ENABLE_MOUSE_INPUT, ENABLE_QUICK_EDIT_MODE, on)
}

/// VT input (`ENABLE_VIRTUAL_TERMINAL_INPUT`): the console hands over the
/// bytes a terminal sends, which the decoder reads, in place of its records;
/// off, as it was before.
pub fn set_virtual_terminal_input(on: bool) -> io::Result<()> {
    VT_INPUT.switch(
        ENABLE_VIRTUAL_TERMINAL_INPUT,
        ENABLE_VIRTUAL_TERMINAL_INPUT,
        0,
        on,
    )
}

/// The console's size, columns then rows.
pub fn size() -> io::Result<(u16, u16)> {
    let size = window_size()?;
    Ok((size.columns, size.rows))
}

/// The console's size in cells, and in pixels where the console draws with
/// a font of its own (conhost): see [`window_size_of`].
pub fn window_size() -> io::Result<WindowSize> {
    window_size_of(open_console("CONOUT$")?.as_handle())
}

/// The size of the window onto the screen buffer `output`: its cells, and
/// its pixels as the cells times the console's font size. Behind ConPTY
/// (Windows Terminal, another terminal hosting the console) the console has
/// no font of its own and reports a width of 0, so the pixels are 0, as
/// where a Unix terminal does not report them.
pub fn window_size_of(output: BorrowedHandle<'_>) -> io::Result<WindowSize> {
    let window = screen_buffer_info(output)?.srWindow;
    let cells = |low: i16, high: i16| u16::try_from(i32::from(high) - i32::from(low) + 1);
    let columns = cells(window.Left, window.Right).map_err(io::Error::other)?;
    let rows = cells(window.Top, window.Bottom).map_err(io::Error::other)?;
    let (width, height) = match font_size(output) {
        Some((x, y)) => (columns.saturating_mul(x), rows.saturating_mul(y)),
        None => (0, 0),
    };
    Ok(WindowSize {
        columns,
        rows,
        width,
        height,
    })
}

/// The console's font size in pixels, a cell's width and height, or `None`
/// where it has none to report: the call fails, or a side is 0 (ConPTY's).
fn font_size(output: BorrowedHandle<'_>) -> Option<(u16, u16)> {
    // SAFETY: CONSOLE_FONT_INFOEX is plain data; zero is a valid value of
    // every field.
    let mut font: CONSOLE_FONT_INFOEX = unsafe { std::mem::zeroed() };
    font.cbSize = u32::try_from(std::mem::size_of::<CONSOLE_FONT_INFOEX>()).ok()?;
    // SAFETY: the handle is borrowed, so open; `font` is a valid out
    // pointer whose size is in `cbSize`. FALSE asks for the font at the
    // window's current size, not a maximized window's.
    if unsafe { GetCurrentConsoleFontEx(raw(output), 0, &mut font) } == 0 {
        return None;
    }
    let x = u16::try_from(font.dwFontSize.X).ok().filter(|&x| x > 0)?;
    let y = u16::try_from(font.dwFontSize.Y).ok().filter(|&y| y > 0)?;
    Some((x, y))
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
        let input = input_to_put_back(console, *input);
        set_console_mode(console.input.as_handle(), input)?;
        set_console_mode(console.output.as_handle(), *output)?;
    }
    Ok(raw.take().map(|(console, _, _)| console))
}

/// [`take_raw`] for a panic hook, which may run while this thread holds the
/// state: it is skipped then, and a failure has nowhere to be reported.
pub fn take_raw_after_panic() -> Option<Console> {
    let (console, input, output) = super::lock_after_panic(&RAW)?.take()?;
    // A panic hook has nowhere to report a failure; the modes stay raw.
    let input = input_to_put_back(&console, input);
    let _ = set_console_mode(console.input.as_handle(), input);
    let _ = set_console_mode(console.output.as_handle(), output);
    Some(console)
}

/// Whether [`enable_raw_mode`] has put the console in raw mode.
pub fn is_raw_mode_enabled() -> bool {
    raw_state().is_some()
}

/// Where a panic hook writes the modes' off sequences: the console's
/// output, duplicated from raw mode's so that it is kept to resume on.
pub fn panic_output(raw: Option<&Console>) -> io::Result<File> {
    let output = match raw {
        Some(console) => console.output.try_clone()?,
        None => open_console("CONOUT$")?,
    };
    Ok(File::from(output))
}
