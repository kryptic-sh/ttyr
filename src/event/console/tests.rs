//! Console key records as key events, with the records Windows sends for
//! each key (virtual-key codes and control key state from winuser.h and
//! wincon.h).

use super::*;

const LEFT_CTRL: u32 = LEFT_CTRL_PRESSED;
const LEFT_ALT: u32 = LEFT_ALT_PRESSED;
const SHIFT: u32 = SHIFT_PRESSED;

fn record(vk: u16, unit: u16, down: bool, state: u32) -> KeyRecord {
    KeyRecord {
        vk,
        scan: 0,
        unit,
        down,
        state,
    }
}

fn press(vk: u16, unit: u16, state: u32) -> Option<KeyEvent> {
    ConsoleKeys::default().key(&record(vk, unit, true, state))
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers).with_encoding(KeyEncoding::Windows)
}

fn unit(c: char) -> u16 {
    u16::try_from(u32::from(c)).unwrap()
}

#[test]
fn a_key_presses_repeats_and_releases() {
    let mut keys = ConsoleKeys::default();
    let a = |down| record(0x41, unit('a'), down, 0);
    let kinds: Vec<_> = [a(true), a(true), a(true), a(false), a(true)]
        .iter()
        .map(|r| keys.key(r).unwrap().kind)
        .collect();
    use KeyEventKind::{Press, Release, Repeat};
    assert_eq!(kinds, [Press, Repeat, Repeat, Release, Press]);
    let released = ConsoleKeys::default().key(&a(false)).unwrap();
    assert_eq!(
        released,
        key(KeyCode::Char('a'), KeyModifiers::NONE).with_kind(Release)
    );
}

#[test]
fn shift_and_caps_lock_letters_read_as_the_decoder_reads_them() {
    let shift_a = press(0x41, unit('A'), SHIFT).unwrap();
    assert_eq!(
        shift_a,
        key(KeyCode::Char('a'), KeyModifiers::SHIFT).with_shifted(Some('A'))
    );
    let caps_a = press(0x41, unit('A'), CAPSLOCK_ON).unwrap();
    assert_eq!(
        caps_a,
        key(KeyCode::Char('a'), KeyModifiers::NONE).with_state(KeyEventState::CAPS_LOCK)
    );
    assert_eq!(caps_a.logical_char(), Some('A'));
    let bang = press(0x31, unit('!'), SHIFT).unwrap();
    assert_eq!(bang, key(KeyCode::Char('!'), KeyModifiers::SHIFT));
}

#[test]
fn ctrl_keys_are_the_key_ctrl_was_held_with() {
    let ctrl = KeyModifiers::CONTROL;
    assert_eq!(
        press(0x41, 0x01, LEFT_CTRL),
        Some(key(KeyCode::Char('a'), ctrl))
    );
    assert_eq!(
        press(0x32, 0x00, LEFT_CTRL),
        Some(key(KeyCode::Char('2'), ctrl))
    );
    // VK_OEM_4, `[` on a US layout: the control code names it.
    assert_eq!(
        press(0xDB, 0x1B, LEFT_CTRL),
        Some(key(KeyCode::Char('['), ctrl))
    );
    assert_eq!(
        press(VK_SPACE, unit(' '), LEFT_CTRL),
        Some(key(KeyCode::Char(' '), ctrl))
    );
    // Where crossterm's console reader lost Ctrl.
    assert_eq!(
        press(VK_RETURN, 0x0A, LEFT_CTRL),
        Some(key(KeyCode::Enter, ctrl))
    );
    assert_eq!(
        press(VK_BACK, 0x7F, LEFT_CTRL),
        Some(key(KeyCode::Backspace, ctrl))
    );
}

#[test]
fn alt_and_alt_gr() {
    assert_eq!(
        press(0x58, unit('x'), LEFT_ALT),
        Some(key(KeyCode::Char('x'), KeyModifiers::ALT))
    );
    // AltGr+Q types @ on a German layout: right Alt with left Ctrl.
    let alt_gr = RIGHT_ALT_PRESSED | LEFT_CTRL_PRESSED;
    assert_eq!(
        press(0x51, unit('@'), alt_gr),
        Some(key(KeyCode::Char('@'), KeyModifiers::NONE))
    );
    // The same state with a control code is Ctrl+Alt.
    assert_eq!(
        press(0x51, 0x11, alt_gr),
        Some(key(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL | KeyModifiers::ALT
        ))
    );
}

#[test]
fn an_alt_code_types_its_character_on_alts_release() {
    let mut keys = ConsoleKeys::default();
    let numpad = |vk, down| record(vk, 0, down, LEFT_ALT);
    let records = [
        record(VK_MENU, 0, true, LEFT_ALT),
        numpad(0x62, true),
        numpad(0x62, false),
        numpad(0x63, true),
        numpad(0x63, false),
        numpad(0x63, true),
        numpad(0x63, false),
        record(VK_MENU, unit('é'), false, 0),
    ];
    let events: Vec<_> = records.iter().filter_map(|r| keys.key(r)).collect();
    assert_eq!(events, [key(KeyCode::Char('é'), KeyModifiers::NONE)]);
}

#[test]
fn modifier_and_lock_keys_alone_are_not_events() {
    for vk in [
        VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN, VK_CAPITAL, VK_NUMLOCK, VK_SCROLL,
    ] {
        assert_eq!(press(vk, 0, 0), None, "{vk:#x}");
    }
}

#[test]
fn a_surrogate_pair_is_one_character() {
    let mut keys = ConsoleKeys::default();
    assert_eq!(keys.key(&record(0, 0xD83D, true, 0)), None);
    assert_eq!(
        keys.key(&record(0, 0xDE00, true, 0)),
        Some(key(KeyCode::Char('😀'), KeyModifiers::NONE))
    );
    // A low half alone is nothing.
    assert_eq!(keys.key(&record(0, 0xDE00, true, 0)), None);
}

#[test]
fn the_keypad_is_marked() {
    let keypad = |event: Option<KeyEvent>| event.unwrap().state.contains(KeyEventState::KEYPAD);
    assert!(keypad(press(0x65, unit('5'), NUMLOCK_ON)));
    assert!(keypad(press(VK_RETURN, 0x0D, ENHANCED_KEY)));
    assert!(!keypad(press(VK_RETURN, 0x0D, 0)));
    // With Num Lock off the keypad's arrows are not enhanced; the
    // dedicated ones are.
    assert!(keypad(press(VK_UP, 0, 0)));
    assert!(!keypad(press(VK_UP, 0, ENHANCED_KEY)));
    assert_eq!(press(VK_CLEAR, 0, 0).unwrap().key, KeyCode::KeypadBegin);
}

#[test]
fn named_keys() {
    assert_eq!(press(0x74, 0, 0).unwrap().key, KeyCode::F(5));
    assert_eq!(press(VK_F24, 0, 0).unwrap().key, KeyCode::F(24));
    assert_eq!(press(VK_TAB, 0x09, SHIFT).unwrap().key, KeyCode::BackTab);
    assert_eq!(press(VK_ESCAPE, 0x1B, 0).unwrap().key, KeyCode::Esc);
    assert_eq!(
        press(VK_DELETE, 0, ENHANCED_KEY).unwrap().key,
        KeyCode::Delete
    );
    assert_eq!(
        press(VK_VOLUME_UP, 0, 0).unwrap().key,
        KeyCode::Media(MediaKeyCode::RaiseVolume)
    );
}
