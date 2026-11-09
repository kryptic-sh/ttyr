//! The guard's record, restore, suspend and panic hook, on a pty.

use std::fs::File;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use rustix::termios::{LocalModes, OptionalActions, tcgetattr, tcsetattr};

use super::{Mode, disable, enable, restore, restore_on_panic, suspend_with};
use crate::event::KeyboardEnhancementFlags as Flags;
use crate::terminal::MouseTracking;
use crate::terminal::unix::{enable_raw_mode_with, is_raw_mode_enabled};
use crate::test_pty::{local_modes, pty, terminal_state};

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

/// Reads `len` bytes from the terminal's side, failing rather than hanging
/// when they do not come.
fn read_back(term: &File, len: usize) -> String {
    let mut term = term.try_clone().unwrap();
    let (send, receive) = mpsc::channel();
    thread::spawn(move || {
        let mut got = vec![0; len];
        let read = term.read_exact(&mut got).map(|()| got);
        let _ = send.send(read);
    });
    let got = receive.recv_timeout(Duration::from_secs(5)).unwrap();
    String::from_utf8(got.unwrap()).unwrap()
}

/// Puts `tty` in raw mode through the guard's own state.
fn raw(tty: &OwnedFd) {
    let tty = tty.try_clone().unwrap();
    enable_raw_mode_with(|| Ok(tty)).unwrap();
}

#[test]
fn restore_turns_off_in_reverse_once() {
    let _state = terminal_state();
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
    let _state = terminal_state();
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
    let _state = terminal_state();
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

#[test]
fn suspend_with_gives_the_terminal_back_and_takes_it_again() {
    let _state = terminal_state();
    let (_term, tty) = pty();
    let cooked = tcgetattr(&tty).unwrap();
    raw(&tty);
    let mut out = Vec::new();
    enable(&mut out, Mode::AlternateScreen).unwrap();
    enable(
        &mut out,
        Mode::KeyboardFlags(Flags::DISAMBIGUATE_ESCAPE_CODES),
    )
    .unwrap();
    enable(&mut out, Mode::KeyboardFlags(Flags::REPORT_EVENT_TYPES)).unwrap();
    out.clear();
    let ran = suspend_with(&mut out, || {
        // Given back: cooked, and every mode off.
        let mut now = tcgetattr(&tty).unwrap();
        assert!(now.local_modes.contains(LocalModes::ICANON));
        assert!(!is_raw_mode_enabled());
        // The program in between changes the terminal's attributes.
        now.local_modes.remove(LocalModes::IEXTEN);
        tcsetattr(&tty, OptionalActions::Now, &now).unwrap();
        "ran"
    })
    .unwrap();
    assert_eq!(ran, "ran");
    assert_eq!(
        text(&out),
        "\x1B[<1u\x1B[?1049l\x1B[?25h\x1B[?1049h\x1B[>2u"
    );
    assert!(is_raw_mode_enabled());
    assert!(
        !tcgetattr(&tty)
            .unwrap()
            .local_modes
            .contains(LocalModes::ICANON)
    );
    // Raw mode took the attributes afresh: restoring keeps the change.
    restore(&mut Vec::new()).unwrap();
    let after = tcgetattr(&tty).unwrap();
    assert!(after.local_modes.contains(LocalModes::ICANON));
    assert!(!after.local_modes.contains(LocalModes::IEXTEN));
    assert!(cooked.local_modes.contains(LocalModes::IEXTEN));
}

#[test]
fn a_panic_restores_the_terminal() {
    let _state = terminal_state();
    let (term, tty) = pty();
    let cooked = tcgetattr(&tty).unwrap();
    restore_on_panic();
    // Installing it again is harmless.
    restore_on_panic();
    raw(&tty);
    let mut out = Vec::new();
    enable(&mut out, Mode::BracketedPaste).unwrap();
    enable(&mut out, Mode::FocusChange).unwrap();
    let panicked = std::panic::catch_unwind(|| panic!("on purpose"));
    assert!(panicked.is_err());
    // Written to the terminal raw mode was on.
    let expected = "\x1B[?1004l\x1B[?2004l\x1B[?25h";
    assert_eq!(read_back(&term, expected.len()), expected);
    let after = tcgetattr(&tty).unwrap();
    assert_eq!(local_modes(&after), local_modes(&cooked));
    assert!(!is_raw_mode_enabled());
    // Nothing left to restore.
    let mut out = Vec::new();
    restore(&mut out).unwrap();
    assert!(out.is_empty());
}
