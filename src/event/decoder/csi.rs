//! CSI sequences: keys in every encoding, the mouse, focus, paste and the
//! terminal's reports.

use super::{Parsed, char_key, legacy_key, parse_alt};
use crate::event::console::KeyRecord;
use crate::event::{
    ColorScheme, Event, KeyCode, KeyEncoding, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
    KeyboardEnhancementFlags, MediaKeyCode, ModeSetting, ModifierKeyCode, MouseButton, MouseEvent,
    MouseEventKind, Report,
};

/// The longest CSI sequence read before it is given up as noise. kitty's
/// associated text makes the longest real ones.
const MAX_CSI: usize = 256;

/// A sequence's parameters: each `;`-separated parameter's `:`-separated
/// values, an empty one `None`.
type Params = Vec<Vec<Option<u32>>>;

/// What starts with `ESC [`, at least one byte after it.
pub(super) fn parse(buf: &[u8], at_end: bool) -> Parsed {
    // The Linux console's F1 to F5: CSI [ A to CSI [ E.
    if buf[2] == b'[' {
        return match buf.get(3) {
            None => Parsed::Incomplete,
            Some(&c @ b'A'..=b'E') => Parsed::key(legacy_key(KeyCode::F(1 + c - b'A'), NONE), 4),
            Some(_) => Parsed::unknown(&buf[..4]),
        };
    }
    // Read no further than the longest sequence, so that where one is given
    // up does not depend on how much has arrived.
    let capped = &buf[..buf.len().min(MAX_CSI + 1)];
    let params_end = 2 + capped[2..]
        .iter()
        .take_while(|b| (0x30..=0x3F).contains(*b))
        .count();
    let final_at = params_end
        + capped[params_end..]
            .iter()
            .take_while(|b| (0x20..=0x2F).contains(*b))
            .count();
    let Some(&last) = capped.get(final_at) else {
        if capped.len() > MAX_CSI {
            return Parsed::unknown(capped);
        }
        // rxvt's Shift+key, `CSI n $`, ends where a standard sequence goes on.
        if at_end && &buf[params_end..] == b"$" {
            return rxvt_shift_key(buf, params_end);
        }
        return Parsed::Incomplete;
    };
    if !(0x40..=0x7E).contains(&last) {
        // Not a sequence after all: ESC [ alone is Alt+[.
        return if final_at == 2 {
            parse_alt(buf, at_end)
        } else {
            Parsed::unknown(&buf[..final_at])
        };
    }
    let len = final_at + 1;
    let (params, intermediates) = (&buf[2..params_end], &buf[params_end..final_at]);
    // X10 mouse, `CSI M Cb Cx Cy`: three raw bytes after the final byte.
    if last == b'M' && params.is_empty() && intermediates.is_empty() {
        return match buf.get(len..len + 3) {
            Some(&[cb, cx, cy]) => x10_mouse(cb, cx, cy).map_or_else(
                || Parsed::unknown(&buf[..len + 3]),
                |mouse| Parsed::Event(Event::Mouse(mouse), len + 3),
            ),
            _ => Parsed::Incomplete,
        };
    }
    match dispatch(params, intermediates, last) {
        Some(Dispatched::Event(event)) => Parsed::Event(event, len),
        Some(Dispatched::PasteStart) => Parsed::PasteStart(len),
        Some(Dispatched::ConsoleKey(record)) => Parsed::ConsoleKey(record, len),
        None => Parsed::unknown(&buf[..len]),
    }
}

const NONE: KeyModifiers = KeyModifiers::NONE;

enum Dispatched {
    Event(Event),
    PasteStart,
    ConsoleKey(KeyRecord),
}

/// Windows Terminal's win32-input-mode key record,
/// `CSI Vk ; Sc ; Uc ; Kd ; Cs ; Rc _`, each field a number and 0 when left
/// out. The repeat count (`Rc`) is not read: Windows Terminal sends a held
/// key as more records, which [`ConsoleKeys`](crate::event::console) reads as
/// repeats.
fn console_record(params: &Params) -> Option<KeyRecord> {
    if params.len() > 6 || params.iter().any(|values| values.len() > 1) {
        return None;
    }
    let field = |i: usize| {
        let value = params
            .get(i)
            .and_then(|values| values.first().copied().flatten());
        u16::try_from(value.unwrap_or(0)).ok()
    };
    let state = params
        .get(4)
        .and_then(|values| values.first().copied().flatten())
        .unwrap_or(0);
    Some(KeyRecord {
        vk: field(0)?,
        scan: field(1)?,
        unit: field(2)?,
        down: match field(3)? {
            0 => false,
            1 => true,
            _ => return None,
        },
        state,
    })
}

/// A whole CSI sequence's meaning, or `None` for one ttyr does not decode.
fn dispatch(params: &[u8], intermediates: &[u8], last: u8) -> Option<Dispatched> {
    let (private, params) = match params.first() {
        Some(&p @ b'<'..=b'?') => (Some(p), &params[1..]),
        _ => (None, params),
    };
    let params = parse_params(params)?;
    let p = |i: usize| {
        params
            .get(i)
            .and_then(|values| values.first().copied().flatten())
    };
    let report = |report| Some(Dispatched::Event(Event::Report(report)));
    let event = match (private, intermediates, last) {
        (None, b"", b'_') => return console_record(&params).map(Dispatched::ConsoleKey),
        (None, b"", b'u') => Event::Key(kitty_key(&params)?),
        (None, b"", b'~') => match p(0) {
            Some(200) => return Some(Dispatched::PasteStart),
            Some(27) if params.len() == 3 => Event::Key(xterm_key(&params)?),
            _ => Event::Key(tilde_key(&params)?),
        },
        (None, b"", b'R') if params.len() == 2 => {
            return report(Report::CursorPosition {
                row: one_based(p(0)?)?,
                column: one_based(p(1)?)?,
            });
        }
        (None, b"", b'A'..=b'H' | b'P'..=b'S' | b'Z') => Event::Key(letter_key(&params, last)?),
        (None, b"", b'I') if params.is_empty() => Event::FocusGained,
        (None, b"", b'O') if params.is_empty() => Event::FocusLost,
        (None, b"", b'M') => Event::Mouse(rxvt_mouse(&params)?),
        (Some(b'<'), b"", b'M' | b'm') => Event::Mouse(sgr_mouse(&params, last == b'm')?),
        (None, b"", b'a'..=b'd') if params.is_empty() => {
            let key = [KeyCode::Up, KeyCode::Down, KeyCode::Right, KeyCode::Left];
            Event::Key(legacy_key(
                key[usize::from(last - b'a')],
                KeyModifiers::SHIFT,
            ))
        }
        (None, b"", b'^' | b'@') if params.len() == 1 => {
            let shift = if last == b'@' {
                KeyModifiers::SHIFT
            } else {
                NONE
            };
            let key = tilde_code(p(0)?)?;
            Event::Key(legacy_key(key, KeyModifiers::CONTROL | shift))
        }
        (None, b"", b't') if p(0) == Some(4) && params.len() == 3 => {
            return report(Report::TextAreaPixels {
                height: u16::try_from(p(1)?).ok()?,
                width: u16::try_from(p(2)?).ok()?,
            });
        }
        (Some(b'?'), b"", b'u') => {
            let flags = u8::try_from(p(0).unwrap_or(0)).ok()?;
            return report(Report::KeyboardEnhancementFlags(
                KeyboardEnhancementFlags::from_bits_retain(flags),
            ));
        }
        (Some(b'?'), b"", b'c') => return report(Report::PrimaryDeviceAttributes(flat(&params)?)),
        (Some(b'>'), b"", b'c') => {
            return report(Report::SecondaryDeviceAttributes(flat(&params)?));
        }
        (Some(b'?'), b"", b'n') if p(0) == Some(997) && params.len() == 2 => {
            return report(Report::ColorScheme(match p(1)? {
                1 => ColorScheme::Dark,
                2 => ColorScheme::Light,
                _ => return None,
            }));
        }
        (None | Some(b'?'), b"$", b'y') if params.len() == 2 => {
            return report(Report::Mode {
                mode: u16::try_from(p(0)?).ok()?,
                private: private.is_some(),
                setting: match p(1)? {
                    0 => ModeSetting::NotRecognized,
                    1 => ModeSetting::Set,
                    2 => ModeSetting::Reset,
                    3 => ModeSetting::PermanentlySet,
                    4 => ModeSetting::PermanentlyReset,
                    _ => return None,
                },
            });
        }
        _ => return None,
    };
    Some(Dispatched::Event(event))
}

/// `1;2:3;;4` as `[[1], [2, 3], [None], [4]]`; `None` if a value is not a
/// number. No parameters at all is an empty list.
fn parse_params(bytes: &[u8]) -> Option<Params> {
    if bytes.is_empty() {
        return Some(Vec::new());
    }
    let text = std::str::from_utf8(bytes).ok()?;
    text.split(';')
        .map(|param| {
            param
                .split(':')
                .map(|value| match value {
                    "" => Some(None),
                    digits if digits.bytes().all(|b| b.is_ascii_digit()) => {
                        digits.parse().ok().map(Some)
                    }
                    _ => None,
                })
                .collect()
        })
        .collect()
}

/// Every parameter's first value, as a report's list of numbers.
fn flat(params: &Params) -> Option<Vec<u16>> {
    params
        .iter()
        .map(|values| u16::try_from(values.first().copied().flatten()?).ok())
        .collect()
}

/// A one-based position, zero-based.
fn one_based(value: u32) -> Option<u16> {
    u16::try_from(value.saturating_sub(1)).ok()
}

/// The modifiers and kind in a key sequence's parameter `mods:kind`: its
/// absence is no modifiers and a press.
fn modifiers_param(
    params: &Params,
    i: usize,
) -> Option<(KeyModifiers, KeyEventState, KeyEventKind)> {
    let values = params.get(i).map_or(&[][..], Vec::as_slice);
    let (modifiers, state) = kitty_modifiers(values.first().copied().flatten().unwrap_or(1));
    let kind = match values.get(1).copied().flatten().unwrap_or(1) {
        1 => KeyEventKind::Press,
        2 => KeyEventKind::Repeat,
        3 => KeyEventKind::Release,
        _ => return None,
    };
    Some((modifiers, state, kind))
}

/// The kitty protocol's (and xterm's legacy keys') modifier value, `1 +`
/// the bits Shift 1, Alt 2, Ctrl 4, Super 8, Hyper 16, Meta 32, Caps Lock 64,
/// Num Lock 128. xterm's own bit 8 is Meta, but only `CSI 27 ; mods ; key ~`
/// is xterm's alone; a legacy key's modifiers are read as kitty sends them.
pub(super) fn kitty_modifiers(value: u32) -> (KeyModifiers, KeyEventState) {
    let bits = value.saturating_sub(1);
    let mut modifiers = NONE;
    let mut state = KeyEventState::NONE;
    for (bit, modifier) in [
        (1, KeyModifiers::SHIFT),
        (2, KeyModifiers::ALT),
        (4, KeyModifiers::CONTROL),
        (8, KeyModifiers::SUPER),
        (16, KeyModifiers::HYPER),
        (32, KeyModifiers::META),
    ] {
        if bits & bit != 0 {
            modifiers |= modifier;
        }
    }
    if bits & 64 != 0 {
        state |= KeyEventState::CAPS_LOCK;
    }
    if bits & 128 != 0 {
        state |= KeyEventState::NUM_LOCK;
    }
    (modifiers, state)
}

/// xterm's modifier value: `1 +` Shift 1, Alt 2, Ctrl 4, Meta 8.
fn xterm_modifiers(value: u32) -> KeyModifiers {
    let bits = value.saturating_sub(1);
    let mut modifiers = NONE;
    for (bit, modifier) in [
        (1, KeyModifiers::SHIFT),
        (2, KeyModifiers::ALT),
        (4, KeyModifiers::CONTROL),
        (8, KeyModifiers::META),
    ] {
        if bits & bit != 0 {
            modifiers |= modifier;
        }
    }
    modifiers
}

/// The kitty protocol's `CSI key:shifted:base ; mods:kind ; text u`.
fn kitty_key(params: &Params) -> Option<KeyEvent> {
    let codes = params.first()?;
    let code = codes.first().copied().flatten()?;
    let (modifiers, lock_state, kind) = modifiers_param(params, 1)?;
    let shift = modifiers.contains(KeyModifiers::SHIFT);
    let mut event = match functional_key(code) {
        Some((key, state)) => KeyEvent::new(key, modifiers).with_state(state),
        None => match control_key(code, shift)? {
            KeyCode::Char(c) => char_key(c, modifiers, KeyEncoding::Kitty),
            key => KeyEvent::new(key, modifiers),
        },
    };
    if let Some(shifted) = codes.get(1).copied().flatten() {
        event.shifted = Some(char::from_u32(shifted)?);
    }
    if let Some(base) = codes.get(2).copied().flatten() {
        event.base_layout = Some(match functional_key(base) {
            Some((key, _)) => key,
            None => control_key(base, false)?,
        });
    }
    if let Some(text) = params.get(2) {
        let text: String = text
            .iter()
            .map(|c| c.and_then(char::from_u32))
            .collect::<Option<_>>()?;
        event.text = (!text.is_empty()).then_some(text);
    }
    event.state |= lock_state;
    Some(event.with_kind(kind).with_encoding(KeyEncoding::Kitty))
}

/// xterm's modifyOtherKeys, `CSI 27 ; mods ; key ~`.
fn xterm_key(params: &Params) -> Option<KeyEvent> {
    let value = |i: usize| params.get(i)?.first().copied().flatten();
    let modifiers = xterm_modifiers(value(1).unwrap_or(1));
    let event = match control_key(value(2)?, modifiers.contains(KeyModifiers::SHIFT))? {
        KeyCode::Char(c) => char_key(c, modifiers, KeyEncoding::Xterm),
        key => KeyEvent::new(key, modifiers),
    };
    Some(event.with_encoding(KeyEncoding::Xterm))
}

/// A code point as a key: the C0 keys by name, Shift+Tab as
/// [`KeyCode::BackTab`], everything else a character.
fn control_key(code: u32, shift: bool) -> Option<KeyCode> {
    Some(match code {
        0 => KeyCode::Null,
        9 if shift => KeyCode::BackTab,
        9 => KeyCode::Tab,
        13 => KeyCode::Enter,
        27 => KeyCode::Esc,
        127 => KeyCode::Backspace,
        _ => KeyCode::Char(char::from_u32(code)?),
    })
}

/// The cursor, editing and function keys ending in a letter: `CSI A`,
/// `CSI 1 ; mods A`, kitty's `CSI 1 ; mods:kind A`.
fn letter_key(params: &Params, last: u8) -> Option<KeyEvent> {
    if params.len() > 2 {
        return None;
    }
    let (mut modifiers, state, kind) = modifiers_param(params, 1)?;
    let key = match last {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'E' => KeyCode::KeypadBegin,
        b'F' => KeyCode::End,
        b'H' => KeyCode::Home,
        b'P'..=b'S' => KeyCode::F(1 + last - b'P'),
        b'Z' => {
            modifiers |= KeyModifiers::SHIFT;
            KeyCode::BackTab
        }
        _ => return None,
    };
    Some(legacy_key(key, modifiers).with_state(state).with_kind(kind))
}

/// `CSI n ; mods:kind ~`: the editing and function keys.
fn tilde_key(params: &Params) -> Option<KeyEvent> {
    if params.len() > 2 {
        return None;
    }
    let key = tilde_code(params.first()?.first().copied().flatten()?)?;
    let (modifiers, state, kind) = modifiers_param(params, 1)?;
    Some(legacy_key(key, modifiers).with_state(state).with_kind(kind))
}

/// The key a `CSI n ~` number names.
fn tilde_code(n: u32) -> Option<KeyCode> {
    let n = u8::try_from(n).ok()?;
    Some(match n {
        1 | 7 => KeyCode::Home,
        2 => KeyCode::Insert,
        3 => KeyCode::Delete,
        4 | 8 => KeyCode::End,
        5 => KeyCode::PageUp,
        6 => KeyCode::PageDown,
        11..=15 => KeyCode::F(n - 10),
        17..=21 => KeyCode::F(n - 11),
        23..=26 => KeyCode::F(n - 12),
        28 | 29 => KeyCode::F(n - 13),
        31..=34 => KeyCode::F(n - 14),
        _ => return None,
    })
}

/// rxvt's Shift+editing or function key, `CSI n $`, the whole of `buf`; its
/// number ends at `params_end`. (`CSI n ^` and `CSI n @`, Ctrl and
/// Ctrl+Shift, end in a final byte and are dispatched.)
fn rxvt_shift_key(buf: &[u8], params_end: usize) -> Parsed {
    let number = std::str::from_utf8(&buf[2..params_end])
        .ok()
        .and_then(|s| s.parse().ok());
    match number.and_then(tilde_code) {
        Some(key) => Parsed::key(legacy_key(key, KeyModifiers::SHIFT), buf.len()),
        None => Parsed::unknown(buf),
    }
}

/// kitty's functional keys, the code points it gives keys with no character
/// (its spec's "Functional key definitions" table).
fn functional_key(code: u32) -> Option<(KeyCode, KeyEventState)> {
    let keypad = |key| Some((key, KeyEventState::KEYPAD));
    let key = match code {
        57399..=57408 => {
            return keypad(KeyCode::Char(char::from_digit(code - 57399, 10)?));
        }
        57409 => return keypad(KeyCode::Char('.')),
        57410 => return keypad(KeyCode::Char('/')),
        57411 => return keypad(KeyCode::Char('*')),
        57412 => return keypad(KeyCode::Char('-')),
        57413 => return keypad(KeyCode::Char('+')),
        57414 => return keypad(KeyCode::Enter),
        57415 => return keypad(KeyCode::Char('=')),
        57416 => return keypad(KeyCode::Char(',')),
        57417 => return keypad(KeyCode::Left),
        57418 => return keypad(KeyCode::Right),
        57419 => return keypad(KeyCode::Up),
        57420 => return keypad(KeyCode::Down),
        57421 => return keypad(KeyCode::PageUp),
        57422 => return keypad(KeyCode::PageDown),
        57423 => return keypad(KeyCode::Home),
        57424 => return keypad(KeyCode::End),
        57425 => return keypad(KeyCode::Insert),
        57426 => return keypad(KeyCode::Delete),
        57427 => return keypad(KeyCode::KeypadBegin),
        57358 => KeyCode::CapsLock,
        57359 => KeyCode::ScrollLock,
        57360 => KeyCode::NumLock,
        57361 => KeyCode::PrintScreen,
        57362 => KeyCode::Pause,
        57363 => KeyCode::Menu,
        57376..=57398 => KeyCode::F(u8::try_from(code - 57376 + 13).ok()?),
        57428..=57440 => KeyCode::Media(
            [
                MediaKeyCode::Play,
                MediaKeyCode::Pause,
                MediaKeyCode::PlayPause,
                MediaKeyCode::Reverse,
                MediaKeyCode::Stop,
                MediaKeyCode::FastForward,
                MediaKeyCode::Rewind,
                MediaKeyCode::TrackNext,
                MediaKeyCode::TrackPrevious,
                MediaKeyCode::Record,
                MediaKeyCode::LowerVolume,
                MediaKeyCode::RaiseVolume,
                MediaKeyCode::MuteVolume,
            ][usize::try_from(code - 57428).ok()?],
        ),
        57441..=57454 => KeyCode::Modifier(
            [
                ModifierKeyCode::LeftShift,
                ModifierKeyCode::LeftControl,
                ModifierKeyCode::LeftAlt,
                ModifierKeyCode::LeftSuper,
                ModifierKeyCode::LeftHyper,
                ModifierKeyCode::LeftMeta,
                ModifierKeyCode::RightShift,
                ModifierKeyCode::RightControl,
                ModifierKeyCode::RightAlt,
                ModifierKeyCode::RightSuper,
                ModifierKeyCode::RightHyper,
                ModifierKeyCode::RightMeta,
                ModifierKeyCode::IsoLevel3Shift,
                ModifierKeyCode::IsoLevel5Shift,
            ][usize::try_from(code - 57441).ok()?],
        ),
        _ => return None,
    };
    Some((key, KeyEventState::NONE))
}

/// A mouse report's button byte: buttons, motion and modifiers.
fn mouse_kind(cb: u32) -> Option<(MouseEventKind, KeyModifiers)> {
    let button = (cb & 0b11) | ((cb & 0b1100_0000) >> 4);
    let dragging = cb & 0b10_0000 != 0;
    let pressed = |b| {
        if dragging {
            MouseEventKind::Drag(b)
        } else {
            MouseEventKind::Down(b)
        }
    };
    let kind = match (button, dragging) {
        (0, _) => pressed(MouseButton::Left),
        (1, _) => pressed(MouseButton::Middle),
        (2, _) => pressed(MouseButton::Right),
        (8, _) => pressed(MouseButton::Back),
        (9, _) => pressed(MouseButton::Forward),
        (3, false) => MouseEventKind::Up(MouseButton::Left),
        (3..=5, true) => MouseEventKind::Moved,
        (4, false) => MouseEventKind::ScrollUp,
        (5, false) => MouseEventKind::ScrollDown,
        (6, false) => MouseEventKind::ScrollLeft,
        (7, false) => MouseEventKind::ScrollRight,
        _ => return None,
    };
    let mut modifiers = NONE;
    for (bit, modifier) in [
        (0b0100, KeyModifiers::SHIFT),
        (0b1000, KeyModifiers::ALT),
        (0b1_0000, KeyModifiers::CONTROL),
    ] {
        if cb & bit != 0 {
            modifiers |= modifier;
        }
    }
    Some((kind, modifiers))
}

/// SGR mouse (1006), `CSI < Cb ; Cx ; Cy M`, `m` for a release.
fn sgr_mouse(params: &Params, release: bool) -> Option<MouseEvent> {
    let [cb, cx, cy] = three(params)?;
    let (mut kind, modifiers) = mouse_kind(cb)?;
    if release && let MouseEventKind::Down(button) = kind {
        kind = MouseEventKind::Up(button);
    }
    Some(MouseEvent {
        kind,
        column: one_based(cx)?,
        row: one_based(cy)?,
        modifiers,
    })
}

/// urxvt mouse (1015), `CSI Cb ; Cx ; Cy M`, Cb offset by 32.
fn rxvt_mouse(params: &Params) -> Option<MouseEvent> {
    let [cb, cx, cy] = three(params)?;
    let (kind, modifiers) = mouse_kind(cb.checked_sub(32)?)?;
    Some(MouseEvent {
        kind,
        column: one_based(cx)?,
        row: one_based(cy)?,
        modifiers,
    })
}

/// X10 mouse, `CSI M Cb Cx Cy`, each byte offset by 32.
fn x10_mouse(cb: u8, cx: u8, cy: u8) -> Option<MouseEvent> {
    let (kind, modifiers) = mouse_kind(u32::from(cb.checked_sub(32)?))?;
    let position = |c: u8| Some(u16::from(c.checked_sub(32)?.saturating_sub(1)));
    Some(MouseEvent {
        kind,
        column: position(cx)?,
        row: position(cy)?,
        modifiers,
    })
}

/// Exactly three single-valued parameters.
fn three(params: &Params) -> Option<[u32; 3]> {
    let [a, b, c] = params.as_slice() else {
        return None;
    };
    let one = |values: &Vec<Option<u32>>| match values.as_slice() {
        [Some(v)] => Some(*v),
        _ => None,
    };
    Some([one(a)?, one(b)?, one(c)?])
}
