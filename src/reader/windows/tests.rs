//! The reader end to end on the Windows console, with records written into
//! its input.

use std::thread;
use std::time::{Duration, Instant};

use crate::event::{
    Event, KeyCode, KeyEncoding, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    ModeSetting, MouseButton, MouseEventKind, Report,
};
use crate::reader::Reader;
use crate::terminal::{Console, Mode, disable, enable, window_top};
use crate::test_console::{console, focus, inject, key, mouse, resize};

/// Longer than any wait a test expects to end, so a hang fails instead.
const LONG: Duration = Duration::from_secs(5);

fn windows_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers).with_encoding(KeyEncoding::Windows)
}

fn legacy(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE).with_encoding(KeyEncoding::Legacy))
}

/// VT input on while it lives, so what is written into the input is a
/// terminal's bytes; off again when the test ends, failed or not.
struct VtInput;

impl VtInput {
    fn on() -> Self {
        enable(&mut Vec::new(), Mode::VirtualTerminalInput).unwrap();
        Self
    }
}

impl Drop for VtInput {
    fn drop(&mut self) {
        disable(&mut Vec::new(), Mode::VirtualTerminalInput).unwrap();
    }
}

/// Writes `text` into the input as VT input hands it over: a key record
/// per UTF-16 unit.
fn type_vt(text: &str) {
    let records: Vec<_> = text
        .encode_utf16()
        .map(|unit| key(0, unit, true, 0))
        .collect();
    inject(&records);
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
fn focus_records_read_as_focus_events_without_vt_input() {
    let _console = console();
    let mut reader = Reader::open().unwrap();
    inject(&[focus(true), focus(false)]);
    assert_eq!(reader.read().unwrap(), Event::FocusGained);
    assert_eq!(reader.read().unwrap(), Event::FocusLost);
}

#[test]
fn with_vt_input_focus_records_are_dropped() {
    let _console = console();
    let mut out = Vec::new();
    enable(&mut out, Mode::VirtualTerminalInput).unwrap();
    let mut reader = Reader::open().unwrap();
    // The terminal reports focus itself (`CSI I`) when asked.
    let mut records = vec![focus(true)];
    records.extend("\x1B[I".encode_utf16().map(|unit| key(0, unit, true, 0)));
    inject(&records);
    assert_eq!(reader.read().unwrap(), Event::FocusGained);
    assert!(!reader.poll(Duration::ZERO).unwrap());
    drop(reader);
    disable(&mut out, Mode::VirtualTerminalInput).unwrap();
}

#[test]
fn an_alt_code_reads_as_a_press_and_a_release() {
    let _console = console();
    let mut reader = Reader::open().unwrap();
    // Alt (0x12, left Alt 0x2) held over keypad 2 3 3, then released with
    // the character: as conhost writes a character the layout lacks.
    let mut records = vec![key(0x12, 0, true, 0x2)];
    for vk in [0x62, 0x63, 0x63] {
        records.push(key(vk, 0, true, 0x2));
        records.push(key(vk, 0, false, 0x2));
    }
    records.push(key(0x12, 0xE9, false, 0)); // 0xE9 is é.
    inject(&records);
    let e = windows_key(KeyCode::Char('é'), KeyModifiers::NONE);
    assert_eq!(reader.read().unwrap(), Event::Key(e.clone()));
    assert_eq!(
        reader.read().unwrap(),
        Event::Key(e.with_kind(KeyEventKind::Release))
    );
    assert!(!reader.poll(Duration::ZERO).unwrap());
}

#[test]
fn records_with_no_key_are_the_consoles_bytes_without_vt_input() {
    let _console = console();
    let mut reader = Reader::open().unwrap();
    // DA1's answer as conhost writes it, then a key typed after it.
    let mut records: Vec<_> = "\x1B[?62;22c"
        .encode_utf16()
        .map(|unit| key(0, unit, true, 0))
        .collect();
    records.push(key(0x41, u16::from(b'a'), true, 0));
    inject(&records);
    assert_eq!(
        reader.read().unwrap(),
        Event::Report(crate::event::Report::PrimaryDeviceAttributes(vec![62, 22]))
    );
    assert_eq!(
        reader.read().unwrap(),
        Event::Key(windows_key(KeyCode::Char('a'), KeyModifiers::NONE))
    );
}

#[test]
fn a_zero_poll_reports_an_event_already_decoded() {
    let _console = console();
    let _vt = VtInput::on();
    let mut reader = Reader::open().unwrap();
    type_vt("ab");
    assert!(reader.poll(LONG).unwrap());
    // Both keys arrived in one read: the second is ready without a wait.
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Char('a')));
    assert!(reader.poll(Duration::ZERO).unwrap());
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Char('b')));
}

#[test]
fn with_no_escape_timeout_a_lone_escape_is_read_at_once() {
    let _console = console();
    let _vt = VtInput::on();
    let mut reader = Reader::open().unwrap();
    type_vt("\x1B");
    assert!(reader.poll(LONG).unwrap());
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Esc));
}

#[test]
fn an_escape_timeout_keeps_a_split_sequence_whole() {
    let _console = console();
    let _vt = VtInput::on();
    let mut reader = Reader::open().unwrap();
    reader.set_escape_timeout(Some(LONG));
    type_vt("\x1B");
    // The ESC arrives and is held, not read as Escape.
    assert!(!reader.poll(Duration::from_millis(100)).unwrap());
    assert!(!reader.poll(Duration::ZERO).unwrap());
    type_vt("[A");
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Up));
}

#[test]
fn an_escape_timeout_reads_a_lone_escape_once_it_passes() {
    let _console = console();
    let _vt = VtInput::on();
    let mut reader = Reader::open().unwrap();
    let timeout = Duration::from_millis(100);
    reader.set_escape_timeout(Some(timeout));
    let start = Instant::now();
    type_vt("\x1B");
    assert!(reader.poll(LONG).unwrap());
    assert!(start.elapsed() >= timeout);
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Esc));
}

#[test]
fn an_escape_timeout_too_long_to_reach_never_flushes() {
    let _console = console();
    let _vt = VtInput::on();
    let mut reader = Reader::open().unwrap();
    reader.set_escape_timeout(Some(Duration::MAX));
    type_vt("\x1B");
    assert!(!reader.poll(Duration::from_millis(100)).unwrap());
    type_vt("[A");
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Up));
}

#[test]
fn a_paste_waits_past_a_flush_for_its_end() {
    let _console = console();
    let _vt = VtInput::on();
    let mut reader = Reader::open().unwrap();
    type_vt("\x1B[200~half");
    // The flush leaves the paste held, and the wait does not spin on it.
    assert!(!reader.poll(Duration::from_millis(100)).unwrap());
    type_vt(" and half\x1B[201~");
    assert_eq!(reader.read().unwrap(), Event::Paste("half and half".into()));
}

#[test]
fn a_long_burst_arrives_whole() {
    let _console = console();
    let _vt = VtInput::on();
    let mut reader = Reader::open().unwrap();
    // Far more records than one read takes, so sequences are split
    // between reads.
    let count = 2000;
    type_vt(&"\x1B[1;5Ah".repeat(count));
    let ctrl_up = Event::Key(
        KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL).with_encoding(KeyEncoding::Legacy),
    );
    for _ in 0..count {
        assert_eq!(reader.read().unwrap(), ctrl_up);
        assert_eq!(reader.read().unwrap(), legacy(KeyCode::Char('h')));
    }
    assert!(!reader.poll(Duration::from_millis(50)).unwrap());
}

#[test]
fn a_second_reader_fails_until_the_first_is_dropped() {
    let _console = console();
    let first = Reader::open().unwrap();
    let err = Reader::open().unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::ResourceBusy);
    drop(first);
    Reader::open().unwrap();
}

#[test]
fn a_waker_outliving_its_reader_does_nothing() {
    let _console = console();
    let reader = Reader::open().unwrap();
    let waker = reader.waker();
    drop(reader);
    waker.wake().unwrap();
}

#[test]
fn collect_answers_keeps_the_keys_typed_meanwhile_and_stops_at_da1() {
    let _console = console();
    let _vt = VtInput::on();
    let mut reader = Reader::open().unwrap();
    type_vt("\x1B[?1ua\x1BP>|kitty(0.39.1)\x1B\\b\x1B[?62;22cz");
    let answers = reader.collect_answers(LONG).unwrap();
    assert!(answers.complete);
    assert_eq!(
        answers.events,
        [
            Event::Report(Report::KeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            )),
            legacy(KeyCode::Char('a')),
            Event::Report(Report::TerminalVersion("kitty(0.39.1)".into())),
            legacy(KeyCode::Char('b')),
            Event::Report(Report::PrimaryDeviceAttributes(vec![62, 22])),
        ]
    );
    let others: Vec<_> = answers.others().cloned().collect();
    assert_eq!(
        others,
        [legacy(KeyCode::Char('a')), legacy(KeyCode::Char('b'))]
    );
    // What came after DA1 is left to read.
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Char('z')));
}

#[test]
fn collect_answers_gives_up_at_the_timeout() {
    let _console = console();
    let _vt = VtInput::on();
    let mut reader = Reader::open().unwrap();
    type_vt("\x1B[?2026;2$yq");
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
    assert_eq!(
        answers.events,
        [Event::Report(mode), legacy(KeyCode::Char('q'))]
    );
}

#[test]
fn collect_answers_returns_a_wake_that_comes_while_it_waits() {
    let _console = console();
    let _vt = VtInput::on();
    let mut reader = Reader::open().unwrap();
    let waker = reader.waker();
    let answering = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        waker.wake().unwrap();
        thread::sleep(Duration::from_millis(50));
        type_vt("\x1B[?62c");
    });
    let answers = reader.collect_answers(LONG).unwrap();
    answering.join().unwrap();
    assert!(answers.complete);
    assert_eq!(
        answers.events,
        [
            Event::Wake,
            Event::Report(Report::PrimaryDeviceAttributes(vec![62])),
        ]
    );
}

#[test]
fn the_reader_and_waker_can_move_between_threads() {
    fn send<T: Send>() {}
    fn send_sync<T: Send + Sync>() {}
    send::<Reader>();
    send_sync::<crate::reader::Waker>();
}
