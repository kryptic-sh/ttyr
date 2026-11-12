//! The Windows side of the reader: the console's input, and the waker's
//! event, waited on together with `WaitForMultipleObjects`.
//!
//! The console hands over records. Key records go through the console key
//! mapping, unless VT input is on (`ENABLE_VIRTUAL_TERMINAL_INPUT`), when
//! their characters are the bytes the terminal sent, read by the decoder as
//! on Unix. A key record no key made (no virtual-key or scan code) is such
//! bytes whatever the mode: the console's answer to a query. Mouse records
//! and resizes become events. Focus records become focus events with VT
//! input off, as crossterm reads them; with it on, the terminal reports
//! focus itself when asked (`?1004h`), and the console's records would
//! report it twice. Menu records are the console's own, which Microsoft's
//! documentation says to ignore.

use std::io;
use std::os::windows::io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::Arc;
use std::time::Duration;

use windows_sys::Win32::Foundation::{WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Console::{
    ENABLE_VIRTUAL_TERMINAL_INPUT, FOCUS_EVENT, GetConsoleWindow, GetNumberOfConsoleInputEvents,
    INPUT_RECORD, KEY_EVENT, MOUSE_EVENT, ReadConsoleInputW, WINDOW_BUFFER_SIZE_EVENT,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, MAPVK_VK_TO_CHAR, MapVirtualKeyExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

use super::Queue;
use crate::event::Event;
use crate::event::console::{ConsoleKeys, ConsoleMouse, KeyRecord, MouseRecord};
use crate::terminal::{Console, window_size_of, window_top};

#[cfg(test)]
mod tests;

/// The most records taken from the console in one read.
const READ_RECORDS: usize = 128;

/// What a Windows reader waits on.
#[derive(Debug)]
pub struct Source {
    console: Console,
    wake: Arc<OwnedHandle>,
    keys: ConsoleKeys,
    mouse: ConsoleMouse,
    /// A high surrogate from VT input, waiting for its low half.
    surrogate: Option<u16>,
}

impl Source {
    /// Reads the process's console, whatever its standard handles are.
    pub fn open() -> io::Result<Self> {
        // SAFETY: no security attributes or name, as allowed; an
        // auto-reset event, not signalled.
        let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a handle CreateEventW just returned is valid, and nothing
        // else owns it.
        let wake = unsafe { OwnedHandle::from_raw_handle(event) };
        Ok(Self {
            console: Console::open()?,
            wake: Arc::new(wake),
            keys: ConsoleKeys::with_layout(layout_char),
            mouse: ConsoleMouse::default(),
            surrogate: None,
        })
    }

    pub fn waker(&self) -> WakeHandle {
        WakeHandle(Arc::clone(&self.wake))
    }

    pub fn size(&self) -> io::Result<(u16, u16)> {
        let size = window_size_of(self.console.output.as_handle())?;
        Ok((size.columns, size.rows))
    }

    /// Waits up to `timeout` (with none, until something comes) and puts
    /// what came into `queue`: whether anything did.
    pub fn wait(&mut self, timeout: Option<Duration>, queue: &mut Queue) -> io::Result<bool> {
        let handles = [
            self.console.input.as_raw_handle(),
            self.wake.as_raw_handle(),
        ];
        // Rounded up, so a wait does not end before its time; one too long
        // for the API waits for ever.
        let millis = timeout.map_or(INFINITE, |d| {
            u32::try_from(d.as_nanos().div_ceil(1_000_000))
                .map_or(INFINITE, |ms| ms.min(INFINITE - 1))
        });
        // SAFETY: both handles are borrowed from `self`, so open for the
        // whole call, and the count matches the array.
        let woke = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, millis) };
        match woke {
            WAIT_TIMEOUT => Ok(false),
            WAIT_FAILED => Err(io::Error::last_os_error()),
            _ if woke == WAIT_OBJECT_0 => {
                self.read_records(queue)?;
                Ok(true)
            }
            _ if woke == WAIT_OBJECT_0 + 1 => {
                queue.push(Event::Wake);
                Ok(true)
            }
            _ => Err(io::Error::other(format!(
                "unexpected wait result {woke:#x}"
            ))),
        }
    }

    /// Takes the records waiting in the console's input into the queue.
    fn read_records(&mut self, queue: &mut Queue) -> io::Result<()> {
        let input = self.console.input.as_raw_handle();
        let mut waiting = 0;
        // SAFETY: the handle is borrowed from `self`; `waiting` is a valid
        // out pointer.
        if unsafe { GetNumberOfConsoleInputEvents(input, &mut waiting) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if waiting == 0 {
            return Ok(());
        }
        let mut records = vec![INPUT_RECORD::default(); READ_RECORDS];
        let mut read = 0;
        let wanted = u32::try_from(records.len())
            .unwrap_or(u32::MAX)
            .min(waiting);
        // SAFETY: `records` has room for `wanted` records; `read` is a valid
        // out pointer. The console has records waiting, so this does not
        // block.
        if unsafe { ReadConsoleInputW(input, records.as_mut_ptr(), wanted, &mut read) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let vt_input = crate::terminal::console_mode(self.console.input.as_handle())?
            & ENABLE_VIRTUAL_TERMINAL_INPUT
            != 0;
        let mut text = Vec::new();
        for record in &records[..usize::try_from(read).unwrap_or(0)] {
            match u32::from(record.EventType) {
                KEY_EVENT => {
                    // SAFETY: the record says it is a key event.
                    let key = unsafe { record.Event.KeyEvent };
                    // SAFETY: a key event's character is a UTF-16 unit.
                    let unit = unsafe { key.uChar.UnicodeChar };
                    let record = KeyRecord {
                        vk: key.wVirtualKeyCode,
                        scan: key.wVirtualScanCode,
                        unit,
                        down: key.bKeyDown != 0,
                        state: key.dwControlKeyState,
                    };
                    if vt_input || record.is_text() {
                        // The terminal's bytes, one unit per key down.
                        if record.down && unit != 0 {
                            text.push(unit);
                        }
                        continue;
                    }
                    self.feed_text(&mut text, queue);
                    for event in self.keys.events(&record) {
                        queue.push(Event::Key(event));
                    }
                }
                FOCUS_EVENT if !vt_input => {
                    self.feed_text(&mut text, queue);
                    // SAFETY: the record says it is a focus event.
                    let focus = unsafe { record.Event.FocusEvent };
                    queue.push(if focus.bSetFocus != 0 {
                        Event::FocusGained
                    } else {
                        Event::FocusLost
                    });
                }
                MOUSE_EVENT => {
                    self.feed_text(&mut text, queue);
                    // SAFETY: the record says it is a mouse event.
                    let mouse = unsafe { record.Event.MouseEvent };
                    let record = MouseRecord {
                        x: mouse.dwMousePosition.X,
                        y: mouse.dwMousePosition.Y,
                        buttons: mouse.dwButtonState,
                        state: mouse.dwControlKeyState,
                        flags: mouse.dwEventFlags,
                    };
                    let top = window_top(self.console.output.as_handle())?;
                    for event in self.mouse.events(&record, top) {
                        queue.push(Event::Mouse(event));
                    }
                }
                WINDOW_BUFFER_SIZE_EVENT => {
                    self.feed_text(&mut text, queue);
                    // The record has the buffer's size; the window's is
                    // what a program draws in.
                    let (columns, rows) = self.size()?;
                    queue.push(Event::Resize(columns, rows));
                }
                _ => {}
            }
        }
        self.feed_text(&mut text, queue);
        Ok(())
    }

    /// Feeds the VT input's UTF-16 units read so far to the decoder, as
    /// UTF-8, keeping a high surrogate whose low half has not come.
    fn feed_text(&mut self, units: &mut Vec<u16>, queue: &mut Queue) {
        if units.is_empty() {
            return;
        }
        let mut all: Vec<u16> = self.surrogate.take().into_iter().collect();
        all.append(units);
        if let Some(&last) = all.last()
            && (0xD800..=0xDBFF).contains(&last)
        {
            self.surrogate = all.pop();
        }
        let text = String::from_utf16_lossy(&all);
        queue.feed(text.as_bytes());
    }
}

/// The character `vk` types unshifted on the console's keyboard layout:
/// the layout of the console window's thread, which follows the user's
/// switches, where the process has a console window; else this thread's.
/// `None` for a dead key or a key with no character.
pub fn layout_char(vk: u16) -> Option<char> {
    // SAFETY: no arguments.
    let window = unsafe { GetConsoleWindow() };
    let thread = if window.is_null() {
        0
    } else {
        // SAFETY: a window handle, which the call only reads; a null
        // process id pointer is allowed.
        unsafe { GetWindowThreadProcessId(window, std::ptr::null_mut()) }
    };
    // SAFETY: any thread id, or 0 for this one.
    let layout = unsafe { GetKeyboardLayout(thread) };
    // SAFETY: plain values in; the layout handle is only read.
    let mapped = unsafe { MapVirtualKeyExW(u32::from(vk), MAPVK_VK_TO_CHAR, layout) };
    // 0 for no character; a dead key has the top bit set (winuser.h's
    // MapVirtualKey docs).
    if mapped == 0 || mapped & 0x8000_0000 != 0 {
        return None;
    }
    char::from_u32(mapped)
}

/// The waker's side: the event the reader waits on.
#[derive(Debug, Clone)]
pub struct WakeHandle(Arc<OwnedHandle>);

impl WakeHandle {
    pub fn wake(&self) -> io::Result<()> {
        // SAFETY: the event handle is owned by the `Arc`, so open.
        if unsafe { SetEvent(self.0.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
