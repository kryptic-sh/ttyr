//! The Windows console's key and mouse records as events: what the console
//! API hands a program (`KEY_EVENT_RECORD`, `MOUSE_EVENT_RECORD`), and the
//! key records Windows Terminal's win32-input-mode writes as
//! `CSI Vk ; Sc ; Uc ; Kd ; Cs ; Rc _`. Plain values in and out, so it is
//! built and tested on every platform.
//!
//! Where crossterm reads these records it drops what ttyr keeps: a key's
//! release and repeat, Ctrl with Enter and Backspace, the keypad, and the
//! back and forward mouse buttons.

use super::key::{single_lower, single_upper, typed_char, typed_with_shift};
use super::{KeyCode, KeyEncoding, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use crate::event::MediaKeyCode;

// Read only by the Windows reader, and tested everywhere.
#[cfg(any(windows, test))]
mod mouse;
#[cfg(test)]
mod tests;

#[cfg(windows)]
pub use mouse::{ConsoleMouse, MouseRecord};

/// A console key record's fields, as `KEY_EVENT_RECORD` has them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyRecord {
    /// The virtual-key code (`wVirtualKeyCode`).
    pub vk: u16,
    /// The scan code (`wVirtualScanCode`).
    pub scan: u16,
    /// The UTF-16 code unit the key typed, or 0 (`uChar`).
    pub unit: u16,
    /// Whether the key went down, not up (`bKeyDown`).
    pub down: bool,
    /// The modifier and lock state (`dwControlKeyState`).
    pub state: u32,
}

impl KeyRecord {
    /// Whether the record is text no key made: no virtual-key code and no
    /// scan code. The console writes the answers to a terminal's queries
    /// into the input so, a character a record, with their bytes as the
    /// characters; they are read as a terminal's bytes, not as keys.
    pub const fn is_text(&self) -> bool {
        self.vk == 0 && self.scan == 0
    }
}

// `dwControlKeyState` bits, fixed by the Windows API (wincon.h).
const RIGHT_ALT_PRESSED: u32 = 0x0001;
const LEFT_ALT_PRESSED: u32 = 0x0002;
const RIGHT_CTRL_PRESSED: u32 = 0x0004;
const LEFT_CTRL_PRESSED: u32 = 0x0008;
const SHIFT_PRESSED: u32 = 0x0010;
const NUMLOCK_ON: u32 = 0x0020;
const CAPSLOCK_ON: u32 = 0x0080;
const ENHANCED_KEY: u32 = 0x0100;

// Virtual-key codes, fixed by the Windows API (winuser.h).
const VK_BACK: u16 = 0x08;
const VK_TAB: u16 = 0x09;
const VK_CLEAR: u16 = 0x0C;
const VK_RETURN: u16 = 0x0D;
const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const VK_PAUSE: u16 = 0x13;
const VK_CAPITAL: u16 = 0x14;
const VK_ESCAPE: u16 = 0x1B;
const VK_SPACE: u16 = 0x20;
const VK_PRIOR: u16 = 0x21;
const VK_NEXT: u16 = 0x22;
const VK_END: u16 = 0x23;
const VK_HOME: u16 = 0x24;
const VK_LEFT: u16 = 0x25;
const VK_UP: u16 = 0x26;
const VK_RIGHT: u16 = 0x27;
const VK_DOWN: u16 = 0x28;
const VK_SNAPSHOT: u16 = 0x2C;
const VK_INSERT: u16 = 0x2D;
const VK_DELETE: u16 = 0x2E;
const VK_LWIN: u16 = 0x5B;
const VK_RWIN: u16 = 0x5C;
const VK_APPS: u16 = 0x5D;
const VK_NUMPAD0: u16 = 0x60;
const VK_NUMPAD9: u16 = 0x69;
const VK_DIVIDE: u16 = 0x6F;
const VK_F1: u16 = 0x70;
const VK_F24: u16 = 0x87;
const VK_NUMLOCK: u16 = 0x90;
const VK_SCROLL: u16 = 0x91;
const VK_VOLUME_MUTE: u16 = 0xAD;
const VK_VOLUME_DOWN: u16 = 0xAE;
const VK_VOLUME_UP: u16 = 0xAF;
const VK_MEDIA_NEXT_TRACK: u16 = 0xB0;
const VK_MEDIA_PREV_TRACK: u16 = 0xB1;
const VK_MEDIA_STOP: u16 = 0xB2;
const VK_MEDIA_PLAY_PAUSE: u16 = 0xB3;
const VK_OEM_1: u16 = 0xBA;
const VK_OEM_PLUS: u16 = 0xBB;
const VK_OEM_COMMA: u16 = 0xBC;
const VK_OEM_MINUS: u16 = 0xBD;
const VK_OEM_PERIOD: u16 = 0xBE;
const VK_OEM_2: u16 = 0xBF;
const VK_OEM_3: u16 = 0xC0;
const VK_OEM_4: u16 = 0xDB;
const VK_OEM_5: u16 = 0xDC;
const VK_OEM_6: u16 = 0xDD;
const VK_OEM_7: u16 = 0xDE;

/// The keyboard layout, as a lookup: the character a key types unshifted,
/// by its virtual-key code, or `None` where the layout has none to give (a
/// dead key, a key with no character).
pub type Layout = fn(u16) -> Option<char>;

/// Turns console key records into key events, keeping what spans records:
/// the key held down (so a second down is a repeat) and the first half of a
/// surrogate pair.
#[derive(Debug, Clone, Default)]
pub struct ConsoleKeys {
    /// The virtual-key code of the key down and not yet up.
    held: Option<u16>,
    /// A high surrogate waiting for its low half.
    surrogate: Option<u16>,
    /// The layout the records were typed on, where it can be read: only
    /// the process at the keyboard can, so not for records that came from
    /// another machine.
    layout: Option<Layout>,
}

impl ConsoleKeys {
    /// Reads records typed on `layout`: a character Shift typed is reported
    /// on the key it was typed with, as kitty reports it (Shift+1 as `1`,
    /// with `!` shifted), and Ctrl with a key the console gives no
    /// character for is that key's character on the layout. Without one,
    /// the character typed is the key, and Ctrl's key is the US layout's.
    // Only the Windows reader has a layout to read; tested everywhere.
    #[cfg(any(windows, test))]
    pub fn with_layout(layout: Layout) -> Self {
        Self {
            layout: Some(layout),
            ..Self::default()
        }
    }

    /// The character `vk` types unshifted on the layout, lower case.
    fn unshifted(&self, vk: u16) -> Option<char> {
        let c = (self.layout?)(vk)?;
        Some(single_lower(c).unwrap_or(c))
    }

    /// The key events `record` is: [`ConsoleKeys::key`]'s, and after an Alt
    /// code's character its release. The console makes Alt codes itself for
    /// characters the keyboard layout lacks, pasted or sent by a terminal,
    /// and no record releases what they type.
    pub fn events(&mut self, record: &KeyRecord) -> impl Iterator<Item = KeyEvent> + use<> {
        let event = self.key(record);
        let release = event
            .as_ref()
            .filter(|_| is_alt_code(record))
            .map(|press| press.clone().with_kind(KeyEventKind::Release));
        event.into_iter().chain(release)
    }

    /// The character a text record's `unit` completes (see
    /// [`KeyRecord::is_text`]): itself, or with the high surrogate before
    /// it, which is held until its low half comes.
    pub fn text(&mut self, unit: u16) -> Option<char> {
        let units = match unit {
            0xD800..=0xDBFF => {
                self.surrogate = Some(unit);
                return None;
            }
            0xDC00..=0xDFFF => [self.surrogate.take()?, unit],
            _ => {
                self.surrogate = None;
                return char::from_u32(u32::from(unit));
            }
        };
        char::decode_utf16(units).next()?.ok()
    }

    /// The key event `record` is, or `None` for a record that is not one on
    /// its own: a modifier or lock key (Shift, Ctrl, Alt, the Windows keys,
    /// Caps Lock, Num Lock, Scroll Lock), a digit typed on the keypad as part
    /// of an Alt code, or half of a surrogate pair.
    pub fn key(&mut self, record: &KeyRecord) -> Option<KeyEvent> {
        let key = self.code(record)?;
        // An Alt code's character comes with Alt's release, but is typed.
        let kind = if is_alt_code(record) {
            KeyEventKind::Press
        } else {
            self.kind(record)
        };
        let mut modifiers = modifiers(record.state);
        if alt_gr_typed(record) {
            modifiers.remove(KeyModifiers::CONTROL | KeyModifiers::ALT);
        }
        let mut event = match key {
            Key::Code(code) => KeyEvent::new(code, modifiers),
            Key::Char(c) => typed_char(c, modifiers),
            Key::Typed(c) => typed_with_shift(c, modifiers),
            Key::Shifted { key, typed } => {
                KeyEvent::new(KeyCode::Char(key), modifiers).with_shifted(Some(typed))
            }
        };
        // The record's own lock state, which `typed_char`'s guess at Caps
        // Lock gives way to.
        event = event.with_kind(kind).with_state(state(record));
        Some(event.with_encoding(KeyEncoding::Windows))
    }

    /// Press, repeat or release, from whether the same key is held.
    fn kind(&mut self, record: &KeyRecord) -> KeyEventKind {
        if !record.down {
            if self.held == Some(record.vk) {
                self.held = None;
            }
            return KeyEventKind::Release;
        }
        if self.held.replace(record.vk) == Some(record.vk) {
            KeyEventKind::Repeat
        } else {
            KeyEventKind::Press
        }
    }

    /// The key, by its virtual-key code where that names one, else by the
    /// character it typed.
    fn code(&mut self, record: &KeyRecord) -> Option<Key> {
        let shift = record.state & SHIFT_PRESSED != 0;
        let code = match record.vk {
            _ if is_alt_code(record) => return self.typed(record.unit),
            VK_SHIFT | VK_CONTROL | VK_MENU | VK_LWIN | VK_RWIN | VK_CAPITAL | VK_NUMLOCK
            | VK_SCROLL => return None,
            VK_NUMPAD0..=VK_NUMPAD9 if is_alt_code_digit(record.state) => return None,
            VK_BACK => KeyCode::Backspace,
            VK_TAB if shift => KeyCode::BackTab,
            VK_TAB => KeyCode::Tab,
            VK_RETURN => KeyCode::Enter,
            VK_ESCAPE => KeyCode::Esc,
            VK_SPACE => KeyCode::Char(' '),
            VK_PRIOR => KeyCode::PageUp,
            VK_NEXT => KeyCode::PageDown,
            VK_END => KeyCode::End,
            VK_HOME => KeyCode::Home,
            VK_LEFT => KeyCode::Left,
            VK_UP => KeyCode::Up,
            VK_RIGHT => KeyCode::Right,
            VK_DOWN => KeyCode::Down,
            VK_INSERT => KeyCode::Insert,
            VK_DELETE => KeyCode::Delete,
            VK_CLEAR => KeyCode::KeypadBegin,
            VK_PAUSE => KeyCode::Pause,
            VK_SNAPSHOT => KeyCode::PrintScreen,
            VK_APPS => KeyCode::Menu,
            VK_F1..=VK_F24 => KeyCode::F(u8::try_from(record.vk - VK_F1 + 1).ok()?),
            VK_VOLUME_MUTE => KeyCode::Media(MediaKeyCode::MuteVolume),
            VK_VOLUME_DOWN => KeyCode::Media(MediaKeyCode::LowerVolume),
            VK_VOLUME_UP => KeyCode::Media(MediaKeyCode::RaiseVolume),
            VK_MEDIA_NEXT_TRACK => KeyCode::Media(MediaKeyCode::TrackNext),
            VK_MEDIA_PREV_TRACK => KeyCode::Media(MediaKeyCode::TrackPrevious),
            VK_MEDIA_STOP => KeyCode::Media(MediaKeyCode::Stop),
            VK_MEDIA_PLAY_PAUSE => KeyCode::Media(MediaKeyCode::PlayPause),
            _ => return self.typed_by(record),
        };
        Some(Key::Code(code))
    }

    /// A key with no code of its own: the character it typed, or with Ctrl,
    /// whose unit is a control code, the key that Ctrl was held with.
    fn typed_by(&mut self, record: &KeyRecord) -> Option<Key> {
        match record.unit {
            0x00..=0x1F => self.control_key(record),
            unit => {
                let typed = self.typed(unit)?;
                Some(self.shifted(record, typed).unwrap_or(typed))
            }
        }
    }

    /// A character Shift typed, as the key it was typed with, where the
    /// layout names that key and it is not the character itself: Shift
    /// went into it. Letters are left to [`typed_char`], and AltGr's
    /// characters are not Shift's.
    fn shifted(&self, record: &KeyRecord, typed: Key) -> Option<Key> {
        let Key::Typed(typed) = typed else {
            return None;
        };
        let shift = record.state & SHIFT_PRESSED != 0;
        let letter = single_lower(typed).is_some() || single_upper(typed).is_some();
        if !shift || letter || typed == ' ' || alt_gr_typed(record) {
            return None;
        }
        let key = self.unshifted(record.vk).filter(|&key| key != typed)?;
        Some(Key::Shifted { key, typed })
    }

    /// The key Ctrl was held with, where the console gives its control
    /// code (`unit` below 0x20) or nothing: a letter or digit by its
    /// virtual-key code, which names it on every layout; otherwise the
    /// character whose control code it is (`0x1B` is Ctrl+\[); and with no
    /// control code (Ctrl+; and the other OEM keys), the key's character on
    /// the layout, or on a US layout where there is none to read.
    fn control_key(&self, record: &KeyRecord) -> Option<Key> {
        let c = match u8::try_from(record.vk).ok()? {
            vk @ (b'A'..=b'Z' | b'0'..=b'9') => char::from(vk).to_ascii_lowercase(),
            _ => match record.unit {
                0 => self
                    .unshifted(record.vk)
                    .or_else(|| us_oem_char(record.vk))?,
                unit => char::from(u8::try_from(unit).ok()? | 0x40).to_ascii_lowercase(),
            },
        };
        Some(Key::Char(c))
    }

    /// The character `unit` completes: itself, or with the high surrogate
    /// before it.
    fn typed(&mut self, unit: u16) -> Option<Key> {
        self.text(unit).map(Key::Typed)
    }
}

/// A key: by code, or by the character it typed.
#[derive(Clone, Copy)]
enum Key {
    Code(KeyCode),
    /// A character naming the key, Shift not in it: a letter or digit by
    /// its virtual-key code, a control code's key.
    Char(char),
    /// The character the key typed, Shift already in it.
    Typed(char),
    /// A character Shift typed (`typed`), and the key's own character on
    /// the layout (`key`).
    Shifted {
        key: char,
        typed: char,
    },
}

/// The unshifted character of an OEM key on a US layout, as winuser.h
/// names them.
const fn us_oem_char(vk: u16) -> Option<char> {
    Some(match vk {
        VK_OEM_1 => ';',
        VK_OEM_PLUS => '=',
        VK_OEM_COMMA => ',',
        VK_OEM_MINUS => '-',
        VK_OEM_PERIOD => '.',
        VK_OEM_2 => '/',
        VK_OEM_3 => '`',
        VK_OEM_4 => '[',
        VK_OEM_5 => '\\',
        VK_OEM_6 => ']',
        VK_OEM_7 => '\'',
        _ => return None,
    })
}

/// Shift, Ctrl and Alt from the control key state.
fn modifiers(state: u32) -> KeyModifiers {
    let mut modifiers = KeyModifiers::NONE;
    if state & SHIFT_PRESSED != 0 {
        modifiers |= KeyModifiers::SHIFT;
    }
    if state & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0 {
        modifiers |= KeyModifiers::CONTROL;
    }
    if state & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED) != 0 {
        modifiers |= KeyModifiers::ALT;
    }
    modifiers
}

/// The lock state, and whether the key is on the keypad.
fn state(record: &KeyRecord) -> KeyEventState {
    let mut state = KeyEventState::NONE;
    if record.state & CAPSLOCK_ON != 0 {
        state |= KeyEventState::CAPS_LOCK;
    }
    if record.state & NUMLOCK_ON != 0 {
        state |= KeyEventState::NUM_LOCK;
    }
    if on_keypad(record) {
        state |= KeyEventState::KEYPAD;
    }
    state
}

/// Whether the key is on the keypad: its digits and operators, its Enter
/// (an enhanced Enter), and with Num Lock off its navigation keys, which
/// share codes with the dedicated ones but are not enhanced.
fn on_keypad(record: &KeyRecord) -> bool {
    let enhanced = record.state & ENHANCED_KEY != 0;
    match record.vk {
        VK_NUMPAD0..=VK_DIVIDE | VK_CLEAR => true,
        VK_RETURN => enhanced,
        VK_PRIOR..=VK_DOWN | VK_INSERT | VK_DELETE => !enhanced,
        _ => false,
    }
}

/// Whether AltGr typed the character: the console reports AltGr as right
/// Alt with left Ctrl, which then are not modifiers of what it typed.
fn alt_gr_typed(record: &KeyRecord) -> bool {
    let alt_gr = RIGHT_ALT_PRESSED | LEFT_CTRL_PRESSED;
    record.state & alt_gr == alt_gr && record.unit >= 0x20 && record.unit != 0x7F
}

/// Whether the record ends an Alt code: Alt's release, with the character
/// the code typed.
const fn is_alt_code(record: &KeyRecord) -> bool {
    record.vk == VK_MENU && !record.down && record.unit != 0
}

/// Whether a keypad digit is part of an Alt code: Alt held alone.
const fn is_alt_code_digit(state: u32) -> bool {
    let alt = state & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED) != 0;
    let other = state & (SHIFT_PRESSED | LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0;
    alt && !other
}
