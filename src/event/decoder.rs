//! Bytes to events, with no I/O.

mod csi;
mod strings;
#[cfg(test)]
mod tests;

use super::console::{ConsoleKeys, KeyRecord};
use super::key::{single_lower, typed_char};
use super::{Event, KeyCode, KeyEncoding, KeyEvent, KeyEventState, KeyModifiers, Report};

const ESC: u8 = 0x1B;

/// What bracketed paste ends with.
const PASTE_END: &[u8] = b"\x1B[201~";

/// The longest bracketed paste held for its end marker before it is given
/// up, so that a paste whose end never comes cannot hold the input, and
/// memory, for ever. Far above any paste a person makes.
const MAX_PASTE: usize = 16 << 20;

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
    /// Inside a bracketed paste, which begins at `start`.
    paste: Option<Paste>,
    /// What spans win32-input-mode's key records: the key held, and half a
    /// surrogate pair.
    console: ConsoleKeys,
    /// Whether the kitty keyboard protocol is on, which decides what a
    /// key's modifier bit 8 is.
    kitty_keyboard: bool,
    /// The release that follows an Alt code's character, which one record
    /// brings with the press.
    release: Option<KeyEvent>,
    /// The bytes win32-input-mode's text records stand for (see
    /// [`KeyRecord::is_text`]), read on their own: a sequence in them is
    /// spread over records, so not in a row in `buf`.
    text: Option<Box<Self>>,
}

impl Decoder {
    /// A decoder with nothing held.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads win32-input-mode's key records as typed on `layout` (see
    /// [`ConsoleKeys::with_layout`]): only for a terminal at this
    /// process's keyboard.
    #[cfg(windows)]
    pub(crate) fn set_console_layout(&mut self, layout: super::console::Layout) {
        self.console = ConsoleKeys::with_layout(layout);
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
    /// paste, which ends only with its end marker. (A paste whose end marker
    /// has not come after many megabytes is given up as
    /// [`Event::Unknown`], its start marker and the bytes held, and what
    /// follows is read as typed.)
    pub fn flush_event(&mut self) -> Option<Event> {
        self.decode(true)
    }

    /// Says whether the kitty keyboard protocol is on: off by default, and
    /// set by the terminal's answer to
    /// [`RequestKeyboardEnhancementFlags`](crate::terminal::RequestKeyboardEnhancementFlags)
    /// as it is decoded.
    ///
    /// It decides what bit 8 of a key's modifier value is (`CSI 1 ; 9 A`):
    /// Super under the kitty protocol, Meta in xterm's encoding, which
    /// terminals use when it is off. A program that turns the protocol on
    /// says so here, or Super arrives as Meta.
    pub fn set_kitty_keyboard(&mut self, on: bool) {
        self.kitty_keyboard = on;
    }

    /// Whether bytes are held that are not an event yet.
    pub fn has_pending(&self) -> bool {
        self.start < self.buf.len() || self.text.as_ref().is_some_and(|text| text.has_pending())
    }

    fn decode(&mut self, at_end: bool) -> Option<Event> {
        let event = self.next(at_end)?;
        if let Event::Report(Report::KeyboardEnhancementFlags(flags)) = &event {
            self.kitty_keyboard = !flags.is_empty();
        }
        Some(event)
    }

    fn next(&mut self, at_end: bool) -> Option<Event> {
        if let Some(release) = self.release.take() {
            return Some(Event::Key(release));
        }
        let input = Input {
            at_end,
            kitty: self.kitty_keyboard,
        };
        loop {
            // Text records' events first: they came before what is left.
            if let Some(text) = &mut self.text {
                text.kitty_keyboard = self.kitty_keyboard;
                if let Some(event) = text.decode(at_end) {
                    return Some(event);
                }
            }
            let pending = &self.buf[self.start..];
            if let Some(paste) = self.paste {
                return self.paste(paste);
            }
            if pending.is_empty() {
                return None;
            }
            match parse(pending, input) {
                Parsed::Event(event, len) => {
                    self.start += len;
                    return Some(event);
                }
                Parsed::PasteStart(len) => {
                    self.paste = Some(Paste {
                        marker: len,
                        scanned: 0,
                    });
                }
                Parsed::ConsoleKey(record, len) if record.is_text() => {
                    self.start += len;
                    if record.down
                        && record.unit != 0
                        && let Some(c) = self.console.text(record.unit)
                    {
                        let text = self.text.get_or_insert_default();
                        text.feed(c.encode_utf8(&mut [0; 4]).as_bytes());
                    }
                }
                Parsed::ConsoleKey(record, len) => {
                    self.start += len;
                    let mut events = self.console.events(&record);
                    if let Some(key) = events.next() {
                        self.release = events.next();
                        return Some(Event::Key(key));
                    }
                }
                Parsed::Incomplete => return None,
            }
        }
    }

    /// The paste, once its end has arrived, or given up as unknown once it
    /// is longer than [`MAX_PASTE`] without one.
    fn paste(&mut self, paste: Paste) -> Option<Event> {
        let pending = &self.buf[self.start..];
        let text = &pending[paste.marker..];
        // Search no further than the longest paste, so that where one is
        // given up does not depend on how much has arrived.
        let capped = &text[..text.len().min(MAX_PASTE + PASTE_END.len())];
        let Some(end) = capped[paste.scanned..]
            .windows(PASTE_END.len())
            .position(|w| w == PASTE_END)
        else {
            if text.len() >= MAX_PASTE + PASTE_END.len() {
                let given_up = pending[..paste.marker + MAX_PASTE].to_vec();
                self.start += given_up.len();
                self.paste = None;
                return Some(Event::Unknown(given_up));
            }
            self.paste = Some(Paste {
                scanned: capped.len().saturating_sub(PASTE_END.len() - 1),
                ..paste
            });
            return None;
        };
        let end = paste.scanned + end;
        let text = String::from_utf8_lossy(&text[..end]).into_owned();
        self.start += paste.marker + end + PASTE_END.len();
        self.paste = None;
        Some(Event::Paste(text))
    }
}

/// A bracketed paste whose end has not arrived.
#[derive(Debug, Clone, Copy)]
struct Paste {
    /// How long its start marker is.
    marker: usize,
    /// How much of the pasted text is known not to hold the end marker's
    /// start.
    scanned: usize,
}

/// What reading the bytes depends on besides them.
#[derive(Debug, Clone, Copy)]
struct Input {
    /// Whether the bytes are all there is, for now.
    at_end: bool,
    /// Whether the kitty keyboard protocol is on.
    kitty: bool,
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
fn parse(buf: &[u8], input: Input) -> Parsed {
    let legacy = |key, modifiers| Parsed::key(legacy_key(key, modifiers), 1);
    match buf[0] {
        ESC => parse_esc(buf, input),
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

/// A character key from bytes that cannot say whether Shift was held: an
/// upper-case letter is taken as typed with Shift (see [`typed_char`]).
fn char_key(c: char, modifiers: KeyModifiers, encoding: KeyEncoding) -> KeyEvent {
    let modifiers = if single_lower(c).is_some() {
        modifiers | KeyModifiers::SHIFT
    } else {
        modifiers
    };
    typed_char(c, modifiers).with_encoding(encoding)
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
fn parse_esc(buf: &[u8], input: Input) -> Parsed {
    let Some(&second) = buf.get(1) else {
        return if input.at_end {
            Parsed::key(legacy_key(KeyCode::Esc, KeyModifiers::NONE), 1)
        } else {
            Parsed::Incomplete
        };
    };
    // ESC [, ESC O, ESC ] and ESC P begin sequences, or are Alt+that key; at
    // the end of the input they are the key.
    let starts_sequence = matches!(second, b'[' | b'O' | b']' | b'P');
    if starts_sequence && buf.len() == 2 {
        return if input.at_end {
            parse_alt(buf, input)
        } else {
            Parsed::Incomplete
        };
    }
    match second {
        b'[' => csi::parse(buf, input),
        b'O' => parse_ss3(buf, input.kitty),
        b']' if buf[2].is_ascii_digit() => strings::parse_osc(buf, input),
        b'P' if buf[2] == b'>' => strings::parse_dcs(buf, input),
        _ => parse_alt(buf, input),
    }
}

/// `ESC` and a key: that key with Alt. `ESC` before anything else is
/// Escape, and what follows is read on its own.
fn parse_alt(buf: &[u8], input: Input) -> Parsed {
    match parse(&buf[1..], input) {
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
fn parse_ss3(buf: &[u8], kitty: bool) -> Parsed {
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
            Some(value) => csi::modifiers_value(value, kitty),
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
        b'E' => {
            keypad = KeyEventState::KEYPAD;
            KeyCode::KeypadBegin
        }
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
