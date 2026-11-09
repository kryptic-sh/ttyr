//! The reader end to end on the Windows console, with records written into
//! its input.

use std::thread;
use std::time::{Duration, Instant};

use crate::event::{
    Event, KeyCode, KeyEncoding, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crate::reader::Reader;
use crate::terminal::{Console, Mode, disable, enable, window_top};
use crate::test_console::{console, inject, key, mouse, resize};

/// Longer than any wait a test expects to end, so a hang fails instead.
const LONG: Duration = Duration::from_secs(5);

fn windows_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers).with_encoding(KeyEncoding::Windows)
}

#[test]
fn key_records_read_as_keys_with_their_releases() {
    let _console = console();
    let mut reader = Reader::open().unwrap();
    // `a`, then Ctrl+Enter (left Ctrl is 0x8 in wincon.h).
    inject(&[
        key(0x41, u16::from(b'a'), true, 0),
        key(0x41, u16::from(b'a'), false, 0),
        key(0x0D, 0x0A, true, 0x8),
    ]);
    let a = windows_key(KeyCode::Char('a'), KeyModifiers::NONE);
    assert_eq!(reader.read().unwrap(), Event::Key(a.clone()));
    assert_eq!(
        reader.read().unwrap(),
        Event::Key(a.with_kind(KeyEventKind::Release))
    );
    assert_eq!(
        reader.read().unwrap(),
        Event::Key(windows_key(KeyCode::Enter, KeyModifiers::CONTROL))
    );
}

#[test]
fn a_timed_out_poll_returns_false_promptly() {
    let _console = console();
    let mut reader = Reader::open().unwrap();
    let wait = Duration::from_millis(50);
    let start = Instant::now();
    assert!(!reader.poll(wait).unwrap());
    let took = start.elapsed();
    assert!(took >= wait && took < LONG, "{took:?}");
    assert!(!reader.poll(Duration::ZERO).unwrap());
}

#[test]
fn a_waker_wakes_a_wait_from_another_thread() {
    let _console = console();
    let mut reader = Reader::open().unwrap();
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
fn a_resize_record_reads_as_the_windows_size() {
    let _console = console();
    let mut reader = Reader::open().unwrap();
    inject(&[resize()]);
    let (columns, rows) = reader.size().unwrap();
    assert_eq!(reader.read().unwrap(), Event::Resize(columns, rows));
}

#[test]
fn mouse_records_read_as_mouse_events() {
    let _console = console();
    let mut reader = Reader::open().unwrap();
    let console = Console::open().unwrap();
    let top = window_top(std::os::windows::io::AsHandle::as_handle(&console.output)).unwrap();
    inject(&[mouse(4, top + 2, 0x1, 0), mouse(4, top + 2, 0x0, 0)]);
    let Event::Mouse(down) = reader.read().unwrap() else {
        panic!("not a mouse event");
    };
    assert_eq!(down.kind, MouseEventKind::Down(MouseButton::Left));
    assert_eq!((down.column, down.row), (4, 2));
    let Event::Mouse(up) = reader.read().unwrap() else {
        panic!("not a mouse event");
    };
    assert_eq!(up.kind, MouseEventKind::Up(MouseButton::Left));
}

#[test]
fn with_vt_input_the_records_are_the_terminals_bytes() {
    let _console = console();
    let mut out = Vec::new();
    enable(&mut out, Mode::VirtualTerminalInput).unwrap();
    let mut reader = Reader::open().unwrap();
    // The console hands VT input over as key records, one per unit.
    let units: Vec<_> = "\x1B[1;5A€"
        .encode_utf16()
        .map(|unit| key(0, unit, true, 0))
        .collect();
    inject(&units);
    let ctrl_up =
        KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL).with_encoding(KeyEncoding::Legacy);
    assert_eq!(reader.read().unwrap(), Event::Key(ctrl_up));
    let euro =
        KeyEvent::new(KeyCode::Char('€'), KeyModifiers::NONE).with_encoding(KeyEncoding::Legacy);
    assert_eq!(reader.read().unwrap(), Event::Key(euro));
    drop(reader);
    disable(&mut out, Mode::VirtualTerminalInput).unwrap();
}

#[test]
fn the_reader_and_waker_can_move_between_threads() {
    fn send<T: Send>() {}
    fn send_sync<T: Send + Sync>() {}
    send::<Reader>();
    send_sync::<crate::reader::Waker>();
}
