//! The guard's record and restore, on every platform; suspend and the panic
//! hook on each platform's terminal.

use super::{Mode, disable, enable, restore};
use crate::event::KeyboardEnhancementFlags as Flags;
use crate::terminal::MouseTracking;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

const ANY: Mode = Mode::Mouse {
    tracking: MouseTracking::Any,
    sgr: true,
};
const NORMAL: Mode = Mode::Mouse {
    tracking: MouseTracking::Normal,
    sgr: true,
};

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

/// The guard's state is per process, and on Windows the mouse mode changes
/// the console's: the tests take turns.
#[cfg(unix)]
fn state() -> std::sync::MutexGuard<'static, ()> {
    crate::test_pty::terminal_state()
}

/// The guard's state is per process, and on Windows the mouse mode changes
/// the console's: the tests take turns.
#[cfg(windows)]
fn state() -> std::sync::MutexGuard<'static, ()> {
    crate::test_console::console()
}

#[test]
fn restore_turns_off_in_reverse_once() {
    let _state = state();
    let mut out = Vec::new();
    enable(&mut out, Mode::AlternateScreen).unwrap();
    enable(&mut out, Mode::BracketedPaste).unwrap();
    // On already: nothing written.
    enable(&mut out, Mode::AlternateScreen).unwrap();
    assert_eq!(text(&out), "\x1B[?1049h\x1B[?2004h");
    out.clear();
    restore(&mut out).unwrap();
    assert_eq!(text(&out), "\x1B[?2004l\x1B[?1049l\x1B[?25h");
    out.clear();
    restore(&mut out).unwrap();
    assert!(out.is_empty());
}

#[test]
fn another_value_replaces_the_mode_in_place() {
    let _state = state();
    let mut out = Vec::new();
    enable(&mut out, ANY).unwrap();
    enable(
        &mut out,
        Mode::KeyboardFlags(Flags::DISAMBIGUATE_ESCAPE_CODES),
    )
    .unwrap();
    out.clear();
    enable(&mut out, NORMAL).unwrap();
    assert_eq!(
        text(&out),
        "\x1B[?1006l\x1B[?1003l\x1B[?1002l\x1B[?1000l\x1B[?1000h\x1B[?1006h"
    );
    out.clear();
    let all = Flags::DISAMBIGUATE_ESCAPE_CODES | Flags::REPORT_EVENT_TYPES;
    enable(&mut out, Mode::KeyboardFlags(all)).unwrap();
    assert_eq!(text(&out), "\x1B[<1u\x1B[>3u");
    out.clear();
    // The mouse stays first, so it goes off last.
    restore(&mut out).unwrap();
    assert_eq!(
        text(&out),
        "\x1B[<1u\x1B[?1006l\x1B[?1003l\x1B[?1002l\x1B[?1000l\x1B[?25h"
    );
}

#[test]
fn disable_turns_off_whatever_value_is_on() {
    let _state = state();
    let mut out = Vec::new();
    enable(&mut out, ANY).unwrap();
    out.clear();
    disable(&mut out, NORMAL).unwrap();
    assert_eq!(text(&out), "\x1B[?1006l\x1B[?1003l\x1B[?1002l\x1B[?1000l");
    out.clear();
    disable(&mut out, NORMAL).unwrap();
    restore(&mut out).unwrap();
    assert!(out.is_empty());
}
