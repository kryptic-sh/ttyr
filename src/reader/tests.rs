//! The reader end to end over a pty: the test writes to the terminal side
//! (the master), as a terminal emulator would, and the reader reads the
//! program side.

use std::fs::File;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use rustix::fs::{Mode, OFlags};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{OptionalActions, Winsize, tcgetattr, tcsetattr, tcsetwinsize};

use super::Reader;
use crate::event::{Event, KeyCode, KeyEncoding, KeyEvent, KeyModifiers};

/// The reader is one per process and `SIGWINCH` goes to the whole process,
/// so the tests take turns.
static SERIAL: Mutex<()> = Mutex::new(());

/// Longer than any wait a test expects to end, so a hang fails instead.
const LONG: Duration = Duration::from_secs(5);

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A pty in raw mode: the terminal's side, and the program's.
fn pty() -> (File, OwnedFd) {
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
    grantpt(&master).unwrap();
    unlockpt(&master).unwrap();
    let name = ptsname(&master, Vec::new()).unwrap();
    let slave = rustix::fs::open(
        name.as_c_str(),
        OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .unwrap();
    let mut termios = tcgetattr(&slave).unwrap();
    termios.make_raw();
    tcsetattr(&slave, OptionalActions::Now, &termios).unwrap();
    (File::from(master), slave)
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
        waker.wake().unwrap();
    });
    let start = Instant::now();
    assert_eq!(reader.read().unwrap(), Event::Wake);
    assert!(start.elapsed() < LONG);
    waking.join().unwrap();
    // The two wakes were one event.
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
