//! The reader end to end on the Windows console, with records written into
//! its input.

use std::sync::MutexGuard;
use std::time::Duration;

use crate::event::{
    Event, KeyCode, KeyEncoding, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    ModifierKeyCode, MouseButton, MouseEventKind, Report,
};
use crate::reader::Reader;
use crate::reader::shared_tests::{LONG, Terminal, legacy, shared_tests};
use crate::terminal::{Console, Mode, disable, enable, window_top};
use crate::test_console::{console, focus, inject, key, mouse, resize};

fn windows_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers).with_encoding(KeyEncoding::Windows)
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

/// The console with VT input on, so what is typed is a terminal's bytes,
/// the test's turn held while it lives.
struct VtConsole {
    // Before the console's turn, so that VT input is off before the next
    // test takes the console.
    _vt: VtInput,
    _console: MutexGuard<'static, ()>,
}

impl VtConsole {
    fn new() -> Self {
        let console = console();
        Self {
            _vt: VtInput::on(),
            _console: console,
        }
    }
}

impl Terminal for VtConsole {
    fn reader(&mut self) -> Reader {
        Reader::open().unwrap()
    }

    fn type_text(&mut self, text: &str) {
        type_vt(text);
    }

    fn typist(&self) -> impl FnMut(&str) + Send + 'static {
        type_vt
    }
}

shared_tests!(VtConsole::new());

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
    let alt = windows_key(
        KeyCode::Modifier(ModifierKeyCode::LeftAlt),
        KeyModifiers::ALT,
    );
    assert_eq!(reader.read().unwrap(), Event::Key(alt));
    let e = windows_key(KeyCode::Char('é'), KeyModifiers::NONE);
    assert_eq!(reader.read().unwrap(), Event::Key(e.clone()));
    assert_eq!(
        reader.read().unwrap(),
        Event::Key(e.with_kind(KeyEventKind::Release))
    );
    // Alt's own release, the same record's.
    let alt_up = windows_key(
        KeyCode::Modifier(ModifierKeyCode::LeftAlt),
        KeyModifiers::NONE,
    )
    .with_kind(KeyEventKind::Release);
    assert_eq!(reader.read().unwrap(), Event::Key(alt_up));
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
fn the_reader_and_waker_can_move_between_threads() {
    fn send<T: Send>() {}
    fn send_sync<T: Send + Sync>() {}
    send::<Reader>();
    send_sync::<crate::reader::Waker>();
}

#[test]
fn the_layout_is_read_from_the_system() {
    use super::layout_char;
    // Space is a space on every layout; Shift types nothing.
    assert_eq!(layout_char(0x20), Some(' '));
    assert_eq!(layout_char(0x10), None);
    // Each letter key types something, whatever the script.
    for vk in 0x41..=0x5A {
        assert!(layout_char(vk).is_some(), "{vk:#x}");
    }
}

#[test]
fn a_character_shift_typed_is_read_on_the_layouts_key() {
    let _console = console();
    let mut reader = Reader::open().unwrap();
    // Shift+1 (VK 0x31, Shift is 0x10 in wincon.h) typing `!`.
    inject(&[key(0x31, u16::from(b'!'), true, 0x10)]);
    let Event::Key(event) = reader.read().unwrap() else {
        panic!("not a key");
    };
    // The key is whatever the machine's layout has unshifted there (`1` on
    // a US layout), unless that is `!` itself.
    let unshifted = super::layout_char(0x31).filter(|&c| c != '!');
    let expected = unshifted.map_or('!', |c| c.to_lowercase().next().unwrap());
    assert_eq!(event.key, KeyCode::Char(expected));
    assert_eq!(event.shifted, Some('!'));
    assert_eq!(event.logical_char(), Some('!'));
}
