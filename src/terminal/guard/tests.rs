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

/// A mode's on and off bytes as xterm's ctlseqs and kitty's protocol give
/// them; a match, so that a new mode fails to compile here until it has
/// them.
const fn sequences(mode: Mode) -> (&'static str, &'static str) {
    match mode {
        Mode::AlternateScreen => ("\x1B[?1049h", "\x1B[?1049l"),
        Mode::Mouse { .. } => (
            "\x1B[?1002h",
            "\x1B[?1006l\x1B[?1003l\x1B[?1002l\x1B[?1000l",
        ),
        Mode::BracketedPaste => ("\x1B[?2004h", "\x1B[?2004l"),
        Mode::FocusChange => ("\x1B[?1004h", "\x1B[?1004l"),
        Mode::ColorSchemeReports => ("\x1B[?2031h", "\x1B[?2031l"),
        Mode::ModifyOtherKeys => ("\x1B[>4;2m", "\x1B[>4m"),
        Mode::KeyboardFlags(_) => ("\x1B[>5u", "\x1B[<1u"),
        Mode::Win32Input => ("\x1B[?9001h", "\x1B[?9001l"),
        // The console's own mode, with no sequence.
        #[cfg(windows)]
        Mode::VirtualTerminalInput => ("", ""),
    }
}

/// One of every mode, the mouse at button level without SGR and the kitty
/// flags at 1 and 4, whose bytes `sequences` gives.
const EVERY_MODE: &[Mode] = &[
    Mode::AlternateScreen,
    Mode::Mouse {
        tracking: MouseTracking::Button,
        sgr: false,
    },
    Mode::BracketedPaste,
    Mode::FocusChange,
    Mode::ColorSchemeReports,
    Mode::ModifyOtherKeys,
    Mode::KeyboardFlags(Flags::DISAMBIGUATE_ESCAPE_CODES.union(Flags::REPORT_ALTERNATE_KEYS)),
    Mode::Win32Input,
    #[cfg(windows)]
    Mode::VirtualTerminalInput,
];

#[test]
fn every_mode_goes_on_and_off_and_is_restored_in_reverse() {
    let _state = state();
    for &mode in EVERY_MODE {
        let (on, off) = sequences(mode);
        let mut out = Vec::new();
        enable(&mut out, mode).unwrap();
        assert_eq!(text(&out), on, "{mode:?}");
        out.clear();
        disable(&mut out, mode).unwrap();
        assert_eq!(text(&out), off, "{mode:?}");
    }
    let mut out = Vec::new();
    for &mode in EVERY_MODE {
        enable(&mut out, mode).unwrap();
    }
    out.clear();
    restore(&mut out).unwrap();
    let mut expected: String = EVERY_MODE.iter().rev().map(|&m| sequences(m).1).collect();
    expected.push_str("\x1B[?25h");
    assert_eq!(text(&out), expected);
}

#[test]
fn a_mode_whose_write_fails_is_not_recorded() {
    let _state = state();
    let mut out = Vec::new();
    enable(&mut out, Mode::FocusChange).unwrap();
    assert!(enable(&mut crate::test_bytes::Broken, Mode::BracketedPaste).is_err());
    // Another value of a mode that is on: the old one stays recorded.
    assert!(enable(&mut crate::test_bytes::Broken, ANY).is_err());
    out.clear();
    restore(&mut out).unwrap();
    assert_eq!(text(&out), "\x1B[?1004l\x1B[?25h");
}

#[test]
fn a_restore_whose_write_fails_still_clears_the_record() {
    let _state = state();
    enable(&mut Vec::new(), Mode::BracketedPaste).unwrap();
    assert!(restore(&mut crate::test_bytes::Broken).is_err());
    let mut out = Vec::new();
    restore(&mut out).unwrap();
    assert!(out.is_empty());
}
