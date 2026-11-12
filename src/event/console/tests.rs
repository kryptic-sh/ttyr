//! Console key records as key events, with the records Windows sends for
//! each key (virtual-key codes and control key state from winuser.h and
//! wincon.h).

use super::*;
use crate::event::KeyChord;

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
        repeat: 1,
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
fn a_records_repeat_count_is_that_many_keys() {
    use KeyEventKind::{Press, Release, Repeat};
    let mut keys = ConsoleKeys::default();
    let kinds = |keys: &mut ConsoleKeys, record| -> Vec<_> {
        keys.events(&record).map(|event| event.kind).collect()
    };
    let a = |down, repeat| KeyRecord {
        repeat,
        ..record(0x41, unit('a'), down, 0)
    };
    assert_eq!(kinds(&mut keys, a(true, 3)), [Press, Repeat, Repeat]);
    assert_eq!(kinds(&mut keys, a(true, 2)), [Repeat, Repeat]);
    // A release is one, whatever its count; 0 is read as 1.
    assert_eq!(kinds(&mut keys, a(false, 4)), [Release]);
    assert_eq!(kinds(&mut keys, a(true, 0)), [Press]);
    // An Alt code's character is typed once, and released.
    let alt_code = KeyRecord {
        repeat: 3,
        ..record(VK_MENU, unit('é'), false, 0)
    };
    // Its own release, then Alt's.
    assert_eq!(kinds(&mut keys, alt_code), [Press, Release, Release]);
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
    // Shift went into `!`: kept as the shifted key, so it is used up.
    let bang = press(0x31, unit('!'), SHIFT).unwrap();
    assert_eq!(
        bang,
        key(KeyCode::Char('!'), KeyModifiers::SHIFT).with_shifted(Some('!'))
    );
    assert_eq!(bang.logical_modifiers(), KeyModifiers::NONE);
    // A title-case letter is not upper case: it is read as typed, as the
    // decoder reads it from a terminal.
    let title = press(0, unit('ǅ'), SHIFT).unwrap();
    assert_eq!(title, key(KeyCode::Char('ǅ'), KeyModifiers::SHIFT));
}

/// A US layout's unshifted characters, for the keys these tests use.
fn us(vk: u16) -> Option<char> {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(u32::from(vk)),
        VK_SPACE => Some(' '),
        _ => us_oem_char(vk),
    }
}

/// A French AZERTY layout's: the digit row's unshifted characters are
/// symbols, and its digits are Shift's.
fn azerty(vk: u16) -> Option<char> {
    match vk {
        0x31 => Some('&'),
        0x32 => Some('é'),
        // VK_OEM_1 is `$` on AZERTY.
        0xBA => Some('$'),
        _ => us(vk),
    }
}

/// A German layout's: VK_OEM_1 is `ü`, which MapVirtualKey reports in
/// upper case, and VK_OEM_6 (´) is a dead key.
fn german(vk: u16) -> Option<char> {
    match vk {
        0xBA => Some('Ü'),
        0xDD => None,
        _ => us(vk),
    }
}

fn press_on(layout: Layout, vk: u16, unit: u16, state: u32) -> Option<KeyEvent> {
    ConsoleKeys::with_layout(layout).key(&record(vk, unit, true, state))
}

#[test]
fn with_the_layout_a_character_shift_typed_is_on_its_key() {
    let shift = KeyModifiers::SHIFT;
    // Shift+1 as kitty's flags 1 and 4 report it: `1`, with `!` shifted.
    let bang = press_on(us, 0x31, unit('!'), SHIFT).unwrap();
    assert_eq!(bang, key(KeyCode::Char('1'), shift).with_shifted(Some('!')));
    assert_eq!(
        bang.chord(),
        KeyChord::new(KeyCode::Char('!'), KeyModifiers::NONE)
    );
    // Alt+Shift+1, as tmxr re-encodes it: the key is `1`, Shift and Alt
    // held, and the chord is Alt+! as from a legacy terminal's `ESC !`.
    let alt = press_on(us, 0x31, unit('!'), SHIFT | LEFT_ALT).unwrap();
    assert_eq!(
        alt,
        key(KeyCode::Char('1'), shift | KeyModifiers::ALT).with_shifted(Some('!'))
    );
    assert_eq!(
        alt.chord(),
        KeyChord::new(KeyCode::Char('!'), KeyModifiers::ALT)
    );
    // On AZERTY the digit is Shift's: Shift+& types 1.
    assert_eq!(
        press_on(azerty, 0x31, unit('1'), SHIFT),
        Some(key(KeyCode::Char('&'), shift).with_shifted(Some('1')))
    );
    // Unchanged: a letter, Space, a key with no Shift, AltGr's character,
    // and a key the layout has no character for.
    assert_eq!(
        press_on(us, 0x41, unit('A'), SHIFT),
        press(0x41, unit('A'), SHIFT)
    );
    assert_eq!(
        press_on(us, VK_SPACE, unit(' '), SHIFT),
        press(VK_SPACE, unit(' '), SHIFT)
    );
    assert_eq!(
        press_on(azerty, 0x31, unit('&'), 0),
        Some(key(KeyCode::Char('&'), KeyModifiers::NONE))
    );
    let alt_gr = RIGHT_ALT_PRESSED | LEFT_CTRL_PRESSED;
    assert_eq!(
        press_on(german, 0x51, unit('@'), alt_gr | SHIFT),
        press(0x51, unit('@'), alt_gr | SHIFT)
    );
    assert_eq!(
        press_on(german, 0xDD, unit('`'), SHIFT),
        press(0xDD, unit('`'), SHIFT)
    );
}

#[test]
fn with_the_layout_ctrl_with_an_oem_key_is_the_layouts_key() {
    let ctrl = KeyModifiers::CONTROL;
    // No control code, so the key on the layout, in lower case.
    assert_eq!(
        press_on(german, 0xBA, 0, LEFT_CTRL),
        Some(key(KeyCode::Char('ü'), ctrl))
    );
    assert_eq!(
        press_on(azerty, 0xBA, 0, LEFT_CTRL),
        Some(key(KeyCode::Char('$'), ctrl))
    );
    // A dead key falls back to the US key.
    assert_eq!(
        press_on(german, 0xDD, 0, LEFT_CTRL),
        Some(key(KeyCode::Char(']'), ctrl))
    );
    // A control code still names the key, and digits and letters are
    // their virtual-key code's, whatever the layout.
    assert_eq!(
        press_on(german, 0xDB, 0x1B, LEFT_CTRL),
        Some(key(KeyCode::Char('['), ctrl))
    );
    assert_eq!(
        press_on(azerty, 0x32, 0, LEFT_CTRL),
        Some(key(KeyCode::Char('2'), ctrl))
    );
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
    // The OEM keys with no control code: the console gives no character,
    // so the key they are on a US layout.
    for (vk, c) in [
        (0xBA, ';'),
        (0xBB, '='),
        (0xBC, ','),
        (0xBD, '-'),
        (0xBE, '.'),
        (0xBF, '/'),
        (0xC0, '`'),
        (0xDB, '['),
        (0xDC, '\\'),
        (0xDD, ']'),
        (0xDE, '\''),
    ] {
        assert_eq!(press(vk, 0, LEFT_CTRL), Some(key(KeyCode::Char(c), ctrl)));
    }
    assert_eq!(
        press(0xBA, 0, LEFT_CTRL | SHIFT),
        Some(key(KeyCode::Char(';'), ctrl | KeyModifiers::SHIFT))
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
    let events: Vec<_> = records.iter().flat_map(|r| keys.events(r)).collect();
    let alt = KeyCode::Modifier(ModifierKeyCode::LeftAlt);
    let e = key(KeyCode::Char('é'), KeyModifiers::NONE);
    assert_eq!(
        events,
        [
            key(alt, KeyModifiers::ALT),
            e.clone(),
            e.with_kind(KeyEventKind::Release),
            key(alt, KeyModifiers::NONE).with_kind(KeyEventKind::Release),
        ]
    );
}

#[test]
fn only_an_alt_code_brings_its_own_release() {
    let mut keys = ConsoleKeys::default();
    let a: Vec<_> = keys.events(&record(0x41, unit('a'), true, 0)).collect();
    assert_eq!(a, [key(KeyCode::Char('a'), KeyModifiers::NONE)]);
    let shift: Vec<_> = keys.events(&record(VK_SHIFT, 0, true, SHIFT)).collect();
    assert_eq!(
        shift,
        [key(
            KeyCode::Modifier(ModifierKeyCode::LeftShift),
            KeyModifiers::SHIFT
        )]
    );
}

#[test]
fn a_record_with_no_key_is_text() {
    assert!(record(0, 0x1B, true, 0).is_text());
    assert!(!record(VK_ESCAPE, 0x1B, true, 0).is_text());
    let scanned = KeyRecord {
        scan: 1,
        ..record(0, 0x1B, true, 0)
    };
    assert!(!scanned.is_text());
    let mut keys = ConsoleKeys::default();
    assert_eq!(keys.text(0xD83D), None);
    assert_eq!(keys.text(0xDE00), Some('😀'));
    assert_eq!(keys.text(unit('a')), Some('a'));
}

#[test]
fn modifier_and_lock_keys_alone_are_their_own_keys() {
    use ModifierKeyCode::{
        LeftAlt, LeftControl, LeftShift, LeftSuper, RightAlt, RightControl, RightShift, RightSuper,
    };
    let scanned = |vk, scan, state| {
        ConsoleKeys::default()
            .key(&KeyRecord {
                scan,
                ..record(vk, 0, true, state)
            })
            .map(|event| event.key)
    };
    let modifier = |m| Some(KeyCode::Modifier(m));
    // Shift: left and right by the scan code (0x2A and 0x36, set 1).
    assert_eq!(scanned(VK_SHIFT, 0x2A, SHIFT), modifier(LeftShift));
    assert_eq!(scanned(VK_SHIFT, 0x36, SHIFT), modifier(RightShift));
    // Ctrl and Alt: the right keys are enhanced.
    assert_eq!(scanned(VK_CONTROL, 0x1D, LEFT_CTRL), modifier(LeftControl));
    assert_eq!(
        scanned(VK_CONTROL, 0x1D, RIGHT_CTRL_PRESSED | ENHANCED_KEY),
        modifier(RightControl)
    );
    assert_eq!(scanned(VK_MENU, 0x38, LEFT_ALT), modifier(LeftAlt));
    assert_eq!(
        scanned(VK_MENU, 0x38, RIGHT_ALT_PRESSED | ENHANCED_KEY),
        modifier(RightAlt)
    );
    // The left and right virtual-key codes name their side.
    for (vk, m) in [
        (VK_LSHIFT, LeftShift),
        (VK_RSHIFT, RightShift),
        (VK_LCONTROL, LeftControl),
        (VK_RCONTROL, RightControl),
        (VK_LMENU, LeftAlt),
        (VK_RMENU, RightAlt),
        (VK_LWIN, LeftSuper),
        (VK_RWIN, RightSuper),
    ] {
        assert_eq!(scanned(vk, 0, 0), modifier(m), "{vk:#x}");
    }
    assert_eq!(scanned(VK_CAPITAL, 0x3A, 0), Some(KeyCode::CapsLock));
    assert_eq!(scanned(VK_NUMLOCK, 0x45, 0), Some(KeyCode::NumLock));
    assert_eq!(scanned(VK_SCROLL, 0x46, 0), Some(KeyCode::ScrollLock));
    // Pressed, held (the console repeats it) and released, with no
    // character and the modifiers as the console reported them.
    let mut keys = ConsoleKeys::default();
    let kinds: Vec<_> = [true, true, false]
        .map(|down| {
            let state = if down { SHIFT } else { 0 };
            keys.key(&record(VK_SHIFT, 0, down, state)).unwrap()
        })
        .into_iter()
        .map(|event| (event.kind, event.modifiers, event.shifted, event.text))
        .collect();
    use KeyEventKind::{Press, Release, Repeat};
    assert_eq!(
        kinds,
        [
            (Press, KeyModifiers::SHIFT, None, None),
            (Repeat, KeyModifiers::SHIFT, None, None),
            (Release, KeyModifiers::NONE, None, None),
        ]
    );
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

#[test]
fn named_key_table() {
    // winuser.h's virtual-key codes, spelled as numbers so that a slip in
    // the constants above shows here.
    let mut table = vec![
        (0x08, KeyCode::Backspace),
        (0x09, KeyCode::Tab),
        (0x0C, KeyCode::KeypadBegin),
        (0x0D, KeyCode::Enter),
        (0x13, KeyCode::Pause),
        (0x1B, KeyCode::Esc),
        (0x20, KeyCode::Char(' ')),
        (0x21, KeyCode::PageUp),
        (0x22, KeyCode::PageDown),
        (0x23, KeyCode::End),
        (0x24, KeyCode::Home),
        (0x25, KeyCode::Left),
        (0x26, KeyCode::Up),
        (0x27, KeyCode::Right),
        (0x28, KeyCode::Down),
        (0x2C, KeyCode::PrintScreen),
        (0x2D, KeyCode::Insert),
        (0x2E, KeyCode::Delete),
        (0x5D, KeyCode::Menu),
        (0xAD, KeyCode::Media(MediaKeyCode::MuteVolume)),
        (0xAE, KeyCode::Media(MediaKeyCode::LowerVolume)),
        (0xAF, KeyCode::Media(MediaKeyCode::RaiseVolume)),
        (0xB0, KeyCode::Media(MediaKeyCode::TrackNext)),
        (0xB1, KeyCode::Media(MediaKeyCode::TrackPrevious)),
        (0xB2, KeyCode::Media(MediaKeyCode::Stop)),
        (0xB3, KeyCode::Media(MediaKeyCode::PlayPause)),
    ];
    // VK_F1 (0x70) to VK_F24 (0x87).
    table.extend((1..=24).map(|n| (0x6F + u16::from(n), KeyCode::F(n))));
    for (vk, expected) in table {
        // Enhanced, so the navigation keys are the dedicated ones.
        let event = press(vk, 0, ENHANCED_KEY).unwrap();
        assert_eq!(event.key, expected, "{vk:#x}");
    }
}
