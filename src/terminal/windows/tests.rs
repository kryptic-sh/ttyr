//! Raw mode, size and the guard's console modes on the Windows console.

use std::os::windows::io::AsHandle;

use windows_sys::Win32::System::Console::{
    ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_MOUSE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_QUICK_EDIT_MODE, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    ENABLE_WINDOW_INPUT,
};

use super::{Console, console_mode, disable_raw_mode, enable_raw_mode, is_raw_mode_enabled, size};
use crate::terminal::{Mode, MouseTracking, disable, enable, restore};
use crate::test_console::console;

fn modes() -> (u32, u32) {
    let console = Console::open().unwrap();
    (
        console_mode(console.input.as_handle()).unwrap(),
        console_mode(console.output.as_handle()).unwrap(),
    )
}

#[test]
fn raw_mode_turns_off_line_input_and_puts_the_modes_back() {
    let _console = console();
    let before = modes();
    enable_raw_mode().unwrap();
    assert!(is_raw_mode_enabled());
    let (input, output) = modes();
    assert_eq!(
        input & (ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT),
        0
    );
    assert_ne!(input & ENABLE_WINDOW_INPUT, 0);
    assert_ne!(output & ENABLE_VIRTUAL_TERMINAL_PROCESSING, 0);
    // A second time keeps the first modes.
    enable_raw_mode().unwrap();
    disable_raw_mode().unwrap();
    disable_raw_mode().unwrap();
    assert!(!is_raw_mode_enabled());
    assert_eq!(modes(), before);
}

#[test]
fn the_size_is_the_windows() {
    let _console = console();
    let (columns, rows) = size().unwrap();
    assert!(columns > 0 && rows > 0, "{columns}x{rows}");
}

#[test]
fn the_guard_sets_the_console_modes_a_mode_needs() {
    let _console = console();
    let before = modes().0;
    let mut out = Vec::new();
    let mouse = Mode::Mouse {
        tracking: MouseTracking::Normal,
        sgr: true,
    };
    enable(&mut out, mouse).unwrap();
    enable(&mut out, Mode::VirtualTerminalInput).unwrap();
    let on = modes().0;
    assert_ne!(on & ENABLE_MOUSE_INPUT, 0);
    assert_ne!(on & ENABLE_VIRTUAL_TERMINAL_INPUT, 0);
    // The sequence is written too, for a terminal behind the console.
    assert_eq!(std::str::from_utf8(&out).unwrap(), "\x1B[?1000h\x1B[?1006h");
    disable(&mut out, Mode::VirtualTerminalInput).unwrap();
    assert_eq!(modes().0 & ENABLE_VIRTUAL_TERMINAL_INPUT, 0);
    restore(&mut out).unwrap();
    // The console's own bits are back, whatever they were (CI's console has
    // mouse input on to start with).
    let mask = ENABLE_MOUSE_INPUT | ENABLE_QUICK_EDIT_MODE | ENABLE_VIRTUAL_TERMINAL_INPUT;
    assert_eq!(modes().0 & mask, before & mask);
}
