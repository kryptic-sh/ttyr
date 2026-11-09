//! The reader end to end over a pty.

use std::io::Write;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use rustix::termios::{Winsize, tcsetwinsize};

use super::Reader;
use crate::event::{
    Event, KeyCode, KeyEncoding, KeyEvent, KeyModifiers, KeyboardEnhancementFlags, ModeSetting,
    Report,
};
use crate::test_pty::raw_pty as pty;

/// The reader is one per process and `SIGWINCH` goes to the whole process,
/// so the tests take turns.
static SERIAL: Mutex<()> = Mutex::new(());

/// Longer than any wait a test expects to end, so a hang fails instead.
const LONG: Duration = Duration::from_secs(5);

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE).with_encoding(KeyEncoding::Legacy))
}

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
fn a_timed_out_poll_returns_false_promptly() {
    let _serial = serial();
    let (_term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    let wait = Duration::from_millis(50);
    let start = Instant::now();
    assert!(!reader.poll(wait).unwrap());
    let took = start.elapsed();
    assert!(took >= wait && took < LONG, "{took:?}");
    assert!(!reader.poll(Duration::ZERO).unwrap());
}

#[test]
fn a_zero_poll_reports_an_event_already_decoded() {
    let _serial = serial();
    let (mut term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    term.write_all(b"ab").unwrap();
    assert!(reader.poll(LONG).unwrap());
    // Both keys arrived in one read: the second is ready without a wait.
    assert_eq!(reader.read().unwrap(), key(KeyCode::Char('a')));
    assert!(reader.poll(Duration::ZERO).unwrap());
    assert_eq!(reader.read().unwrap(), key(KeyCode::Char('b')));
}

#[test]
fn with_no_escape_timeout_a_lone_escape_is_read_at_once() {
    let _serial = serial();
    let (mut term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    term.write_all(b"\x1B").unwrap();
    assert!(reader.poll(LONG).unwrap());
    assert_eq!(reader.read().unwrap(), key(KeyCode::Esc));
}

#[test]
fn an_escape_timeout_keeps_a_split_sequence_whole() {
    let _serial = serial();
    let (mut term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    reader.set_escape_timeout(Some(LONG));
    term.write_all(b"\x1B").unwrap();
    // The ESC arrives and is held, not read as Escape.
    assert!(!reader.poll(Duration::from_millis(100)).unwrap());
    assert!(!reader.poll(Duration::ZERO).unwrap());
    term.write_all(b"[A").unwrap();
    assert_eq!(reader.read().unwrap(), key(KeyCode::Up));
}

#[test]
fn an_escape_timeout_reads_a_lone_escape_once_it_passes() {
    let _serial = serial();
    let (mut term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    let timeout = Duration::from_millis(100);
    reader.set_escape_timeout(Some(timeout));
    let start = Instant::now();
    term.write_all(b"\x1B").unwrap();
    assert!(reader.poll(LONG).unwrap());
    assert!(start.elapsed() >= timeout);
    assert_eq!(reader.read().unwrap(), key(KeyCode::Esc));
}

#[test]
fn a_paste_waits_past_a_flush_for_its_end() {
    let _serial = serial();
    let (mut term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    term.write_all(b"\x1B[200~half").unwrap();
    // The flush leaves the paste held, and the wait does not spin on it.
    assert!(!reader.poll(Duration::from_millis(100)).unwrap());
    term.write_all(b" and half\x1B[201~").unwrap();
    assert_eq!(reader.read().unwrap(), Event::Paste("half and half".into()));
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
    // Wait until both are in, then read both, in either order.
    thread::sleep(Duration::from_millis(50));
    let mut events = vec![reader.read().unwrap()];
    while reader.poll(Duration::from_millis(50)).unwrap() {
        events.push(reader.read().unwrap());
    }
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(events.contains(&key(KeyCode::Char('x'))));
    assert!(events.iter().any(|e| matches!(e, Event::Resize(..))));
}

#[test]
fn a_waker_wakes_a_wait_from_another_thread() {
    let _serial = serial();
    let (_term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    let waker = reader.waker();
    let waking = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        waker.wake().unwrap();
        waker
    });
    let start = Instant::now();
    assert_eq!(reader.read().unwrap(), Event::Wake);
    assert!(start.elapsed() < LONG);
    let waker = waking.join().unwrap();
    assert!(!reader.poll(Duration::ZERO).unwrap());
    // Wakes before the reader next waits are one event.
    waker.wake().unwrap();
    waker.wake().unwrap();
    assert!(reader.poll(Duration::ZERO).unwrap());
    assert_eq!(reader.read().unwrap(), Event::Wake);
    assert!(!reader.poll(Duration::ZERO).unwrap());
}

#[test]
fn a_waker_outliving_its_reader_does_nothing() {
    let _serial = serial();
    let (_term, tty) = pty();
    let reader = Reader::from_fd(tty).unwrap();
    let waker = reader.waker();
    drop(reader);
    waker.wake().unwrap();
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

#[test]
fn collect_answers_gives_up_at_the_timeout() {
    let _serial = serial();
    let (mut term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    term.write_all(b"\x1B[?2026;2$yq").unwrap();
    let timeout = Duration::from_millis(100);
    let start = Instant::now();
    let answers = reader.collect_answers(timeout).unwrap();
    assert!(start.elapsed() >= timeout && start.elapsed() < LONG);
    assert!(!answers.complete);
    let mode = Report::Mode {
        mode: 2026,
        private: true,
        setting: ModeSetting::Reset,
    };
    assert_eq!(answers.reports().collect::<Vec<_>>(), [&mode]);
    assert_eq!(
        answers.events,
        [Event::Report(mode.clone()), key(KeyCode::Char('q'))]
    );
}

#[test]
fn collect_answers_returns_a_wake_that_comes_while_it_waits() {
    let _serial = serial();
    let (mut term, tty) = pty();
    let mut reader = Reader::from_fd(tty).unwrap();
    let waker = reader.waker();
    let answering = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        waker.wake().unwrap();
        thread::sleep(Duration::from_millis(50));
        term.write_all(b"\x1B[?62c").unwrap();
        term
    });
    let answers = reader.collect_answers(LONG).unwrap();
    let _term = answering.join().unwrap();
    assert!(answers.complete);
    assert_eq!(
        answers.events,
        [
            Event::Wake,
            Event::Report(Report::PrimaryDeviceAttributes(vec![62])),
        ]
    );
}
