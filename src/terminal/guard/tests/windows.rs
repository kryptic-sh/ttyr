//! Suspend and the panic hook on the Windows console.

use std::os::windows::io::AsHandle;

use windows_sys::Win32::System::Console::{
    ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_MOUSE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_QUICK_EDIT_MODE, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_WRAP_AT_EOL_OUTPUT,
};

use super::super::{Mode, enable, restore, restore_on_panic, resume_after_panic, suspend_with};
use super::{NORMAL, text};
use crate::event::KeyboardEnhancementFlags as Flags;
use crate::terminal::windows::{
    Console, console_mode, enable_raw_mode, is_raw_mode_enabled, set_console_mode,
};
use crate::test_console::console;

/// The console's input and output modes.
fn modes() -> (u32, u32) {
    let console = Console::open().unwrap();
    (
        console_mode(console.input.as_handle()).unwrap(),
        console_mode(console.output.as_handle()).unwrap(),
    )
}

/// The input bits the guard's modes change.
const MODE_BITS: u32 = ENABLE_MOUSE_INPUT | ENABLE_QUICK_EDIT_MODE | ENABLE_VIRTUAL_TERMINAL_INPUT;

#[test]
fn suspend_with_gives_the_console_back_and_takes_it_again() {
    let _console = console();
    let before = modes();
    enable_raw_mode().unwrap();
    let mut out = Vec::new();
    enable(&mut out, Mode::AlternateScreen).unwrap();
    enable(&mut out, Mode::VirtualTerminalInput).unwrap();
    enable(
        &mut out,
        Mode::KeyboardFlags(Flags::DISAMBIGUATE_ESCAPE_CODES),
    )
    .unwrap();
    out.clear();
    let ran = suspend_with(&mut out, || {
        // Given back: line input, no VT input, raw mode off.
        let (input, output) = modes();
        assert_ne!(input & ENABLE_LINE_INPUT, 0);
        assert_eq!(input & MODE_BITS, before.0 & MODE_BITS);
        assert!(!is_raw_mode_enabled());
        // The program in between changes the console's output mode.
        let console = Console::open().unwrap();
        set_console_mode(
            console.output.as_handle(),
            output ^ ENABLE_WRAP_AT_EOL_OUTPUT,
        )
        .unwrap();
        "ran"
    })
    .unwrap();
    assert_eq!(ran, "ran");
    assert_eq!(
        text(&out),
        "\x1B[<1u\x1B[?1049l\x1B[?25h\x1B[?1049h\x1B[>1u"
    );
    assert!(is_raw_mode_enabled());
    let (input, _) = modes();
    assert_eq!(input & ENABLE_LINE_INPUT, 0);
    assert_ne!(input & ENABLE_VIRTUAL_TERMINAL_INPUT, 0);
    // Raw mode took the modes afresh: restoring keeps the change.
    restore(&mut Vec::new()).unwrap();
    let after = modes();
    assert_eq!(after.0, before.0);
    assert_eq!(
        after.1 & ENABLE_WRAP_AT_EOL_OUTPUT,
        (before.1 ^ ENABLE_WRAP_AT_EOL_OUTPUT) & ENABLE_WRAP_AT_EOL_OUTPUT
    );
    let console = Console::open().unwrap();
    set_console_mode(console.output.as_handle(), before.1).unwrap();
}

#[test]
fn a_panic_restores_the_console() {
    let _console = console();
    let before = modes();
    restore_on_panic();
    // Installing it again is harmless.
    restore_on_panic();
    // The modes' console bits without raw mode, which would otherwise put
    // the input mode back on its own.
    let mut out = Vec::new();
    enable(&mut out, NORMAL).unwrap();
    enable(&mut out, Mode::VirtualTerminalInput).unwrap();
    assert_ne!(modes().0 & MODE_BITS, before.0 & MODE_BITS);
    let panicked = std::panic::catch_unwind(|| panic!("on purpose"));
    assert!(panicked.is_err());
    assert_eq!(modes().0 & MODE_BITS, before.0 & MODE_BITS);
    // And with raw mode.
    enable_raw_mode().unwrap();
    enable(&mut out, Mode::BracketedPaste).unwrap();
    let panicked = std::panic::catch_unwind(|| panic!("on purpose"));
    assert!(panicked.is_err());
    assert!(!is_raw_mode_enabled());
    // Turning Quick Edit off and on again may leave ENABLE_EXTENDED_FLAGS
    // reported, so the bits raw mode and the modes change are compared.
    let raw_bits = MODE_BITS | ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT;
    assert_eq!(modes().0 & raw_bits, before.0 & raw_bits);
    assert_eq!(modes().1, before.1);
    // Nothing left to restore.
    let mut out = Vec::new();
    restore(&mut out).unwrap();
    assert!(out.is_empty());
}

/// A panic the test catches, with the guard's hook installed.
fn caught_panic() {
    restore_on_panic();
    let panicked = std::panic::catch_unwind(|| panic!("on purpose"));
    assert!(panicked.is_err());
}

#[test]
fn resume_after_panic_takes_the_console_back() {
    let _console = console();
    let before = modes();
    enable_raw_mode().unwrap();
    let mut out = Vec::new();
    enable(&mut out, Mode::BracketedPaste).unwrap();
    enable(&mut out, NORMAL).unwrap();
    caught_panic();
    assert!(!is_raw_mode_enabled());
    assert_eq!(modes().0 & MODE_BITS, before.0 & MODE_BITS);
    // Back in one call: raw mode, then the modes in the order they were
    // turned on, the mouse's console bits with it.
    let mut out = Vec::new();
    assert!(resume_after_panic(&mut out).unwrap());
    assert_eq!(text(&out), "\x1B[?2004h\x1B[?1000h\x1B[?1006h");
    assert!(is_raw_mode_enabled());
    let (input, _) = modes();
    assert_eq!(input & ENABLE_LINE_INPUT, 0);
    assert_ne!(input & ENABLE_MOUSE_INPUT, 0);
    // Once.
    let mut out = Vec::new();
    assert!(!resume_after_panic(&mut out).unwrap());
    assert!(out.is_empty());
    restore(&mut out).unwrap();
    let raw_bits = MODE_BITS | ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT;
    assert_eq!(modes().0 & raw_bits, before.0 & raw_bits);
}

#[test]
fn a_restore_after_a_panic_leaves_nothing_to_resume() {
    let _console = console();
    enable_raw_mode().unwrap();
    enable(&mut Vec::new(), Mode::BracketedPaste).unwrap();
    caught_panic();
    restore(&mut Vec::new()).unwrap();
    let mut out = Vec::new();
    assert!(!resume_after_panic(&mut out).unwrap());
    assert!(out.is_empty());
    assert!(!is_raw_mode_enabled());
}

#[test]
fn with_nothing_on_at_the_panic_there_is_nothing_to_resume() {
    let _console = console();
    caught_panic();
    let mut out = Vec::new();
    assert!(!resume_after_panic(&mut out).unwrap());
    assert!(out.is_empty());
    assert!(!is_raw_mode_enabled());
}

/// Runs `f` with the process's console gone, as when its window closed,
/// then gives the process a console again.
fn without_a_console<T>(f: impl FnOnce() -> T) -> T {
    use windows_sys::Win32::System::Console::{AllocConsole, FreeConsole};
    // SAFETY: no arguments; the tests that use the console hold its lock.
    assert_ne!(unsafe { FreeConsole() }, 0);
    let result = f();
    // SAFETY: no arguments.
    unsafe { AllocConsole() };
    result
}

#[test]
fn a_console_that_fails_leaves_an_enabled_mode_unwritten_and_unrecorded() {
    let _console = console();
    let mut out = Vec::new();
    without_a_console(|| assert!(enable(&mut out, NORMAL).is_err()));
    assert!(out.is_empty());
    // Not recorded: nothing to turn off.
    restore(&mut out).unwrap();
    assert!(out.is_empty());
}

#[test]
fn a_console_that_fails_still_writes_the_off_sequences_on_restore() {
    let _console = console();
    let mut out = Vec::new();
    enable(&mut out, Mode::BracketedPaste).unwrap();
    enable(&mut out, NORMAL).unwrap();
    out.clear();
    without_a_console(|| assert!(restore(&mut out).is_err()));
    assert_eq!(
        text(&out),
        "\x1B[?1006l\x1B[?1003l\x1B[?1002l\x1B[?1000l\x1B[?2004l\x1B[?25h"
    );
    // The record is cleared all the same.
    out.clear();
    restore(&mut out).unwrap();
    assert!(out.is_empty());
}

#[test]
fn modes_turned_on_before_raw_mode_are_off_after_restore() {
    let _console = console();
    let before = modes();
    let mut out = Vec::new();
    // The other order from crossterm's: the modes, then raw mode, whose
    // snapshot then has their bits on.
    enable(&mut out, NORMAL).unwrap();
    enable(&mut out, Mode::VirtualTerminalInput).unwrap();
    enable_raw_mode().unwrap();
    assert_ne!(modes().0 & MODE_BITS, before.0 & MODE_BITS);
    restore(&mut out).unwrap();
    assert!(!is_raw_mode_enabled());
    assert_eq!(modes().0 & MODE_BITS, before.0 & MODE_BITS);
    // And from the panic hook.
    enable(&mut out, NORMAL).unwrap();
    enable(&mut out, Mode::VirtualTerminalInput).unwrap();
    enable_raw_mode().unwrap();
    restore_on_panic();
    let panicked = std::panic::catch_unwind(|| panic!("on purpose"));
    assert!(panicked.is_err());
    assert_eq!(modes().0 & MODE_BITS, before.0 & MODE_BITS);
    restore(&mut out).unwrap();
}
