//! The Windows console for the tests: allocated when the process has none,
//! with records written into its input as a user's keys and mouse would be.

use std::os::windows::io::{AsHandle, AsRawHandle};
use std::sync::{Mutex, MutexGuard, PoisonError};

use windows_sys::Win32::System::Console::{
    AllocConsole, COORD, FlushConsoleInputBuffer, INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT,
    KEY_EVENT_RECORD, KEY_EVENT_RECORD_0, MOUSE_EVENT, MOUSE_EVENT_RECORD,
    WINDOW_BUFFER_SIZE_EVENT, WINDOW_BUFFER_SIZE_RECORD, WriteConsoleInputW,
};

use crate::terminal::Console;

/// The console is one per process, so the tests that use it take turns;
/// each starts with its input empty.
pub fn console() -> MutexGuard<'static, ()> {
    static CONSOLE: Mutex<()> = Mutex::new(());
    let guard = CONSOLE.lock().unwrap_or_else(PoisonError::into_inner);
    // SAFETY: no arguments; it fails, harmlessly, when the process has a
    // console already.
    unsafe { AllocConsole() };
    let input = Console::open().unwrap().input;
    // SAFETY: the handle is open for the call.
    assert_ne!(unsafe { FlushConsoleInputBuffer(input.as_raw_handle()) }, 0);
    guard
}

/// Writes `records` into the console's input.
pub fn inject(records: &[INPUT_RECORD]) {
    let input = Console::open().unwrap().input;
    let mut written = 0;
    // SAFETY: the handle is open, `records` holds `len` records, and
    // `written` is a valid out pointer.
    let ok = unsafe {
        WriteConsoleInputW(
            input.as_handle().as_raw_handle(),
            records.as_ptr(),
            u32::try_from(records.len()).unwrap(),
            &mut written,
        )
    };
    assert_ne!(ok, 0, "{}", std::io::Error::last_os_error());
    assert_eq!(written as usize, records.len());
}

/// A key record.
pub fn key(vk: u16, unit: u16, down: bool, state: u32) -> INPUT_RECORD {
    INPUT_RECORD {
        EventType: u16::try_from(KEY_EVENT).unwrap(),
        Event: INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: i32::from(down),
                wRepeatCount: 1,
                wVirtualKeyCode: vk,
                wVirtualScanCode: 0,
                uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
                dwControlKeyState: state,
            },
        },
    }
}

/// A mouse record at a column and screen buffer row.
pub fn mouse(x: i16, y: i16, buttons: u32, flags: u32) -> INPUT_RECORD {
    INPUT_RECORD {
        EventType: u16::try_from(MOUSE_EVENT).unwrap(),
        Event: INPUT_RECORD_0 {
            MouseEvent: MOUSE_EVENT_RECORD {
                dwMousePosition: COORD { X: x, Y: y },
                dwButtonState: buttons,
                dwControlKeyState: 0,
                dwEventFlags: flags,
            },
        },
    }
}

/// A resize record.
pub fn resize() -> INPUT_RECORD {
    INPUT_RECORD {
        EventType: u16::try_from(WINDOW_BUFFER_SIZE_EVENT).unwrap(),
        Event: INPUT_RECORD_0 {
            WindowBufferSizeEvent: WINDOW_BUFFER_SIZE_RECORD {
                dwSize: COORD { X: 80, Y: 25 },
            },
        },
    }
}
