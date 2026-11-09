//! Bytes to events, with no I/O.

mod csi;
mod strings;
#[cfg(test)]
mod tests;

use super::console::{ConsoleKeys, KeyRecord};
use super::{Event, KeyCode, KeyEncoding, KeyEvent, KeyEventState, KeyModifiers};

const ESC: u8 = 0x1B;

/// What bracketed paste ends with.
const PASTE_END: &[u8] = b"\x1B[201~";

/// Reads the bytes a terminal sends as [`Event`]s.
///
/// Feed it bytes as they are read, then take events until it has no more. A
/// sequence split across reads is held until the rest arrives, never misread.
///
/// A lone `ESC` is ambiguous: it is the Escape key, or the start of a
/// sequence whose rest has not arrived yet. [`Decoder::next_event`] holds it;
/// [`Decoder::flush_event`] takes the bytes so far as all there is, so it
/// reads a lone `ESC` as Escape and `ESC` with one byte after it as Alt+that
/// key. When to flush is the caller's choice: at once when nothing more is
/// waiting to be read, or after an escape timeout.
///
/// ```
/// use ttyr::event::{Decoder, Event, KeyCode, KeyEvent, KeyModifiers};
///
/// let mut decoder = Decoder::new();
/// decoder.feed(b"\x1B[97;5u\x1B");
/// let ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
/// assert_eq!(decoder.next_event(), Some(Event::Key(ctrl_a)));
/// assert_eq!(decoder.next_event(), None);
/// let Some(Event::Key(esc)) = decoder.flush_event() else { panic!() };
/// assert_eq!(esc.key, KeyCode::Esc);
/// ```
#[derive(Debug, Default, Clone)]
pub struct Decoder {
    buf: Vec<u8>,
    /// Where the undecoded bytes start in `buf`.
    start: usize,
    /// Inside a bracketed paste: how much of the pasted text has been searched
    /// for its end.
    paste_scanned: Option<usize>,
    /// What spans win32-input-mode's key records: the key held, and half a
    /// surrogate pair.
    console: ConsoleKeys,
}

impl Decoder {
    /// A decoder with nothing held.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds bytes read from the terminal.
    pub fn feed(&mut self, bytes: &[u8]) {
        if self.start == self.buf.len() {
            self.buf.clear();
            self.start = 0;
        } else if self.start > self.buf.len() / 2 {
            self.buf.drain(..self.start);
            self.start = 0;
        }
        self.buf.extend_from_slice(bytes);
    }

    /// The next whole event, or `None` until more bytes arrive.
    pub fn next_event(&mut self) -> Option<Event> {
        self.decode(false)
    }

    /// The next event, taking the bytes fed so far as all there is: a lone
    /// `ESC` is Escape, and `ESC` with one byte after it is Alt+that key.
    /// A longer sequence that has begun is still held, as is a bracketed
    /// paste, which ends only with its end marker.
    pub fn flush_event(&mut self) -> Option<Event> {
        self.decode(true)
    }

    /// Whether bytes are held that are not an event yet.
    pub fn has_pending(&self) -> bool {
        self.start < self.buf.len()
    }

    fn decode(&mut self, at_end: bool) -> Option<Event> {
        loop {
            let pending = &self.buf[self.start..];
            if let Some(scanned) = self.paste_scanned {
                return self.paste(scanned);
            }
            if pending.is_empty() {
                return None;
            }
            match parse(pending, at_end) {
                Parsed::Event(event, len) => {
                    self.start += len;
                    return Some(event);
                }
                Parsed::PasteStart(len) => {
                    self.start += len;
                    self.paste_scanned = Some(0);
                }
                Parsed::ConsoleKey(record, len) => {
                    self.start += len;
                    if let Some(key) = self.console.key(&record) {
                        return Some(Event::Key(key));
                    }
                }
                Parsed::Incomplete => return None,
            }
        }
    }

    /// The paste, once its end has arrived. `scanned` bytes of it are known
    /// not to hold the end marker's start.
    fn paste(&mut self, scanned: usize) -> Option<Event> {
        let pending = &self.buf[self.start..];
        let Some(end) = pending[scanned..]
            .windows(PASTE_END.len())
            .position(|w| w == PASTE_END)
        else {
            self.paste_scanned = Some(pending.len().saturating_sub(PASTE_END.len() - 1));
            return None;
        };
        let end = scanned + end;
        let text = String::from_utf8_lossy(&pending[..end]).into_owned();
        self.start += end + PASTE_END.len();
        self.paste_scanned = None;
        Some(Event::Paste(text))
    }
}

/// What the bytes at the front of the input are.
#[derive(Debug, PartialEq)]
enum Parsed {
    /// An event, and how many bytes it took.
    Event(Event, usize),
    /// A bracketed paste begins, after this many bytes.
    PasteStart(usize),
    /// A Windows console key record (win32-input-mode), and how many bytes
    /// it took; the decoder's [`ConsoleKeys`] makes it an event, or not.
    ConsoleKey(KeyRecord, usize),
    /// The start of something whose rest has not arrived.
    Incomplete,
}

impl Parsed {
    fn key(event: KeyEvent, len: usize) -> Self {
        Self::Event(Event::Key(event), len)
    }

    fn unknown(bytes: &[u8]) -> Self {
        Self::Event(Event::Unknown(bytes.to_vec()), bytes.len())
    }
}

/// The first event in `buf`, which is not empty.
fn parse(buf: &[u8], at_end: bool) -> Parsed {
    let legacy = |key, modifiers| Parsed::key(legacy_key(key, modifiers), 1);
    match buf[0] {
        ESC => parse_esc(buf, at_end),
        b'\r' => legacy(KeyCode::Enter, KeyModifiers::NONE),
        b'\t' => legacy(KeyCode::Tab, KeyModifiers::NONE),
        0x7F => legacy(KeyCode::Backspace, KeyModifiers::NONE),
        0 => legacy(KeyCode::Char(' '), KeyModifiers::CONTROL),
        // Ctrl+A to Ctrl+Z; ^H, ^J and the rest included as what they are.
        c @ 0x01..=0x1A => legacy(
            KeyCode::Char(char::from(c - 1 + b'a')),
            KeyModifiers::CONTROL,
        ),
        // ^\ ^] ^^ ^_, as Ctrl+4 to Ctrl+7 send them.
        c @ 0x1C..=0x1F => legacy(
            KeyCode::Char(char::from(c - 0x1C + b'4')),
            KeyModifiers::CONTROL,
        ),
        _ => parse_utf8(buf),
    }
}

/// A key from a legacy sequence.
fn legacy_key(key: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(key, modifiers).with_encoding(KeyEncoding::Legacy)
}

/// A character key: an upper-case letter as its lower case with Shift, the
/// letter it typed kept as the shifted key.
fn char_key(c: char, modifiers: KeyModifiers, encoding: KeyEncoding) -> KeyEvent {
    let event = match single_lower(c) {
        Some(lower) => KeyEvent::new(KeyCode::Char(lower), modifiers | KeyModifiers::SHIFT)
            .with_shifted(Some(c)),
        None => KeyEvent::new(KeyCode::Char(c), modifiers),
    };
    event.with_encoding(encoding)
}

/// `c`'s lower case, where `c` is upper case and its lower case is one
/// character.
fn single_lower(c: char) -> Option<char> {
    if !c.is_uppercase() {
        return None;
    }
    let mut lower = c.to_lowercase();
    let l = lower.next()?;
    (lower.next().is_none() && l != c).then_some(l)
}

/// A UTF-8 character, or the bytes that are not one.
fn parse_utf8(buf: &[u8]) -> Parsed {
    let head = &buf[..buf.len().min(4)];
    let valid = match std::str::from_utf8(head) {
        Ok(s) => s,
        Err(e) if e.valid_up_to() > 0 => {
            std::str::from_utf8(&head[..e.valid_up_to()]).unwrap_or_default()
        }
        Err(e) => {
            return match e.error_len() {
                Some(len) => Parsed::unknown(&buf[..len]),
                None => Parsed::Incomplete,
            };
        }
    };
    match valid.chars().next() {
        Some(c) => Parsed::key(
            char_key(c, KeyModifiers::NONE, KeyEncoding::Legacy),
            c.len_utf8(),
        ),
        None => Parsed::Incomplete,
    }
}

/// What starts with `ESC`.
fn parse_esc(buf: &[u8], at_end: bool) -> Parsed {
    let Some(&second) = buf.get(1) else {
        return if at_end {
            Parsed::key(legacy_key(KeyCode::Esc, KeyModifiers::NONE), 1)
        } else {
            Parsed::Incomplete
        };
    };
    // ESC [, ESC O, ESC ] and ESC P begin sequences, or are Alt+that key; at
    // the end of the input they are the key.
    let starts_sequence = matches!(second, b'[' | b'O' | b']' | b'P');
    if starts_sequence && buf.len() == 2 {
        return if at_end {
            parse_alt(buf, at_end)
        } else {
            Parsed::Incomplete
        };
    }
    match second {
        b'[' => csi::parse(buf, at_end),
        b'O' => parse_ss3(buf),
        b']' if buf[2].is_ascii_digit() => strings::parse_osc(buf),
        b'P' if buf[2] == b'>' => strings::parse_dcs(buf),
        _ => parse_alt(buf, at_end),
    }
}

/// `ESC` and a key: that key with Alt. `ESC` before anything else is
/// Escape, and what follows is read on its own.
fn parse_alt(buf: &[u8], at_end: bool) -> Parsed {
    match parse(&buf[1..], at_end) {
        Parsed::Event(Event::Key(mut key), len) => {
            key.modifiers |= KeyModifiers::ALT;
            Parsed::key(key, len + 1)
        }
        Parsed::Incomplete => Parsed::Incomplete,
        Parsed::Event(..) | Parsed::PasteStart(_) | Parsed::ConsoleKey(..) => {
            Parsed::key(legacy_key(KeyCode::Esc, KeyModifiers::NONE), 1)
        }
    }
}

/// SS3, `ESC O [mods] final`: cursor keys, F1 to F4 and the keypad in
/// application mode, and rxvt's Ctrl+arrows.
fn parse_ss3(buf: &[u8]) -> Parsed {
    let digits = buf[2..].iter().take_while(|b| b.is_ascii_digit()).count();
    let Some(&last) = buf.get(2 + digits) else {
        return if digits > 2 {
            Parsed::unknown(buf)
        } else {
            Parsed::Incomplete
        };
    };
    let len = 3 + digits;
    let (mut modifiers, state) = match &buf[2..2 + digits] {
        [] => (KeyModifiers::NONE, KeyEventState::NONE),
        mods => match std::str::from_utf8(mods).ok().and_then(|s| s.parse().ok()) {
            Some(value) => csi::kitty_modifiers(value),
            None => return Parsed::unknown(&buf[..len]),
        },
    };
    let mut keypad = KeyEventState::NONE;
    let key = match last {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'E' => KeyCode::KeypadBegin,
        b'P'..=b'S' => KeyCode::F(1 + last - b'P'),
        b'a'..=b'd' => {
            modifiers |= KeyModifiers::CONTROL;
            [KeyCode::Up, KeyCode::Down, KeyCode::Right, KeyCode::Left][usize::from(last - b'a')]
        }
        b'M' => {
            keypad = KeyEventState::KEYPAD;
            KeyCode::Enter
        }
        b'X' | b'j'..=b'y' => {
            keypad = KeyEventState::KEYPAD;
            KeyCode::Char(match last {
                b'X' => '=',
                b'j' => '*',
                b'k' => '+',
                b'l' => ',',
                b'm' => '-',
                b'n' => '.',
                b'o' => '/',
                digit => char::from(digit - b'p' + b'0'),
            })
        }
        _ => return Parsed::unknown(&buf[..len]),
    };
    let event = legacy_key(key, modifiers).with_state(state | keypad);
    Parsed::key(event, len)
}
