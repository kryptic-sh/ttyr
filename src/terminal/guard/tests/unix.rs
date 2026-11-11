//! Suspend and the panic hook on a pty.

use std::fs::File;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use rustix::termios::{LocalModes, OptionalActions, tcgetattr, tcsetattr};

use super::super::{Mode, enable, restore, restore_on_panic, resume_after_panic, suspend_with};
use super::text;
use crate::event::KeyboardEnhancementFlags as Flags;
use crate::terminal::unix::{enable_raw_mode_with, is_raw_mode_enabled};
use crate::test_pty::{local_modes, pty, terminal_state};

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

/// A panic the test catches, with the guard's hook installed.
fn caught_panic() {
    restore_on_panic();
    let panicked = std::panic::catch_unwind(|| panic!("on purpose"));
    assert!(panicked.is_err());
}

#[test]
fn resume_after_panic_takes_the_terminal_back() {
    let _state = terminal_state();
    let (term, tty) = pty();
    let cooked = tcgetattr(&tty).unwrap();
    raw(&tty);
    let mut out = Vec::new();
    enable(&mut out, Mode::BracketedPaste).unwrap();
    enable(&mut out, Mode::FocusChange).unwrap();
    caught_panic();
    let expected = "\x1B[?1004l\x1B[?2004l\x1B[?25h";
    assert_eq!(read_back(&term, expected.len()), expected);
    assert_eq!(local_modes(&tcgetattr(&tty).unwrap()), local_modes(&cooked));
    assert!(!is_raw_mode_enabled());
    // Back in one call: raw mode on the same terminal, then the modes in
    // the order they were turned on.
    let mut out = Vec::new();
    assert!(resume_after_panic(&mut out).unwrap());
    assert_eq!(text(&out), "\x1B[?2004h\x1B[?1004h");
    assert!(is_raw_mode_enabled());
    assert!(
        !tcgetattr(&tty)
            .unwrap()
            .local_modes
            .contains(LocalModes::ICANON)
    );
    // Once.
    let mut out = Vec::new();
    assert!(!resume_after_panic(&mut out).unwrap());
    assert!(out.is_empty());
    restore(&mut out).unwrap();
    assert_eq!(text(&out), "\x1B[?1004l\x1B[?2004l\x1B[?25h");
}

#[test]
fn a_restore_after_a_panic_leaves_nothing_to_resume() {
    let _state = terminal_state();
    let (_term, tty) = pty();
    raw(&tty);
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
    let _state = terminal_state();
    caught_panic();
    let mut out = Vec::new();
    assert!(!resume_after_panic(&mut out).unwrap());
    assert!(out.is_empty());
    assert!(!is_raw_mode_enabled());
}

#[test]
fn a_mode_turned_on_between_panic_and_resume_is_not_written_twice() {
    let _state = terminal_state();
    let (_term, tty) = pty();
    raw(&tty);
    enable(&mut Vec::new(), Mode::BracketedPaste).unwrap();
    enable(&mut Vec::new(), Mode::FocusChange).unwrap();
    caught_panic();
    let mut out = Vec::new();
    enable(&mut out, Mode::FocusChange).unwrap();
    out.clear();
    assert!(resume_after_panic(&mut out).unwrap());
    assert_eq!(text(&out), "\x1B[?2004h");
    out.clear();
    // Recorded in the order they came back.
    restore(&mut out).unwrap();
    assert_eq!(text(&out), "\x1B[?2004l\x1B[?1004l\x1B[?25h");
}
