//! The reader end to end over a pty.

use std::fs::File;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use rustix::termios::{Winsize, tcsetwinsize};

use super::Reader;
use super::shared_tests::{LONG, Terminal, legacy as key, shared_tests};
use crate::event::{
    Event, KeyCode, KeyEncoding, KeyEvent, KeyModifiers, KeyboardEnhancementFlags, Report,
};
use crate::test_pty::raw_pty as pty;

/// The reader is one per process and `SIGWINCH` goes to the whole process,
/// so the tests take turns.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A pty in raw mode, the test's turn held while it lives.
struct Pty {
    term: File,
    tty: Option<OwnedFd>,
    _serial: MutexGuard<'static, ()>,
}

impl Pty {
    fn new() -> Self {
        let serial = serial();
        let (term, tty) = pty();
        Self {
            term,
            tty: Some(tty),
            _serial: serial,
        }
    }
}

impl Terminal for Pty {
    fn reader(&mut self) -> Reader {
        Reader::from_fd(self.tty.take().unwrap()).unwrap()
    }

    fn type_text(&mut self, text: &str) {
        self.term.write_all(text.as_bytes()).unwrap();
    }

    fn typist(&self) -> impl FnMut(&str) + Send + 'static {
        let mut term = self.term.try_clone().unwrap();
        move |text| term.write_all(text.as_bytes()).unwrap()
    }
}

shared_tests!(Pty::new());

#[test]
fn reads_keys_as_they_arrive() {
    let _serial = serial();
    let (mut term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    term.write_all(b"\x1B[A\x1B[97;5u").unwrap();
    assert_eq!(reader.read().unwrap(), key(KeyCode::Up));
    let ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert_eq!(reader.read().unwrap(), Event::Key(ctrl_a));
}

#[test]
fn sigwinch_during_a_wait_delivers_the_new_size() {
    let _serial = serial();
    let (term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    let size = Winsize {
        ws_row: 33,
        ws_col: 111,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    tcsetwinsize(&term, size).unwrap();
    assert_eq!(reader.size().unwrap(), (111, 33));
    let raiser = thread::spawn(|| {
        thread::sleep(Duration::from_millis(50));
        signal_hook::low_level::raise(signal_hook::consts::SIGWINCH).unwrap();
    });
    assert!(reader.poll(LONG).unwrap());
    assert_eq!(reader.read().unwrap(), Event::Resize(111, 33));
    raiser.join().unwrap();
}

#[test]
fn sigwinch_with_input_keeps_both() {
    let _serial = serial();
    let (mut term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    term.write_all(b"x").unwrap();
    signal_hook::low_level::raise(signal_hook::consts::SIGWINCH).unwrap();
    // Both, in either order, however long they take to land; then nothing.
    let mut events = Vec::new();
    while events.len() < 2 && reader.poll(LONG).unwrap() {
        events.push(reader.read().unwrap());
    }
    assert!(!reader.poll(Duration::from_millis(50)).unwrap());
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(events.contains(&key(KeyCode::Char('x'))));
    assert!(events.iter().any(|e| matches!(e, Event::Resize(..))));
}

#[test]
fn a_long_burst_arrives_whole() {
    let _serial = serial();
    let (mut term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    // Far more than one read, and than the pty holds: the writer blocks
    // until the reader drains it.
    let burst: Vec<u8> = b"\x1B[1;5Ah".repeat(8 * 1024);
    let count = burst.len() / b"\x1B[1;5Ah".len();
    let writer = thread::spawn(move || term.write_all(&burst).map(|()| term));
    let ctrl_up = Event::Key(
        KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL).with_encoding(KeyEncoding::Legacy),
    );
    for _ in 0..count {
        assert_eq!(reader.read().unwrap(), ctrl_up);
        assert_eq!(reader.read().unwrap(), key(KeyCode::Char('h')));
    }
    let _term = writer.join().unwrap().unwrap();
    assert!(!reader.poll(Duration::from_millis(50)).unwrap());
}

#[test]
fn a_second_reader_fails_until_the_first_is_dropped() {
    let _serial = serial();
    let (_term, tty) = pty();
    let (_term2, tty2) = pty();
    let first = Reader::from_fd(tty).unwrap();
    let err = Reader::from_fd(tty2.try_clone().unwrap()).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::ResourceBusy);
    drop(first);
    Reader::from_fd(tty2).unwrap();
}

#[test]
fn a_closed_terminal_is_an_error_not_a_hang() {
    let _serial = serial();
    let (term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    drop(term);
    assert!(reader.poll(LONG).is_err());
}

#[test]
fn the_reader_and_waker_can_move_between_threads() {
    fn send<T: Send>() {}
    fn send_sync<T: Send + Sync>() {}
    send::<Reader>();
    send_sync::<super::Waker>();
}

#[test]
fn collect_answers_keeps_the_keys_typed_meanwhile_and_stops_at_da1() {
    let _serial = serial();
    let (mut term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    reader.waker().wake().unwrap();
    term.write_all(b"\x1B[?1ua\x1BP>|kitty(0.39.1)\x1B\\b\x1B[?62;22cz")
        .unwrap();
    let answers = reader.collect_answers(LONG).unwrap();
    assert!(answers.complete);
    // The reports among the keys, in the order they came.
    let in_order: Vec<_> = answers
        .events
        .iter()
        .filter(|e| **e != Event::Wake)
        .cloned()
        .collect();
    assert_eq!(
        in_order,
        [
            Event::Report(Report::KeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            )),
            key(KeyCode::Char('a')),
            Event::Report(Report::TerminalVersion("kitty(0.39.1)".into())),
            key(KeyCode::Char('b')),
            Event::Report(Report::PrimaryDeviceAttributes(vec![62, 22])),
        ]
    );
    // The keys typed between the answers, in order; the wake, which came
    // with them in one wait, is kept, before DA1 or after it.
    let mut rest = Vec::new();
    while reader.poll(Duration::from_millis(50)).unwrap() {
        rest.push(reader.read().unwrap());
    }
    let wakes = |events: &[Event]| events.iter().filter(|e| **e == Event::Wake).count();
    assert_eq!(wakes(&answers.events) + wakes(&rest), 1);
    let keys = |events: &[Event]| -> Vec<Event> {
        events
            .iter()
            .filter(|e| **e != Event::Wake)
            .cloned()
            .collect()
    };
    let others: Vec<_> = answers.others().cloned().collect();
    assert_eq!(
        keys(&others),
        [key(KeyCode::Char('a')), key(KeyCode::Char('b'))]
    );
    assert_eq!(answers.reports().count(), 3);
    // What came after DA1 is left to read.
    assert_eq!(keys(&rest), [key(KeyCode::Char('z'))]);
}
