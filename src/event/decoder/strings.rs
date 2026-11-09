//! The string sequences terminals answer with: OSC colour answers and
//! XTVERSION's DCS.

use super::{ESC, Parsed, parse_alt};
use crate::event::{ColorSlot, Event, Report, TerminalColor};

/// The longest string sequence read before it is given up as noise.
const MAX_STRING: usize = 1024;

/// The most digits an OSC number has before `;` (OSC 104 among them); more,
/// and `ESC ]` was Alt+] followed by typing.
const MAX_OSC_DIGITS: usize = 3;

/// Where a string sequence's body ends.
enum StringEnd {
    /// It ends with BEL or ST: the text is `body[..text]`, and the sequence,
    /// its two-byte introducer included, takes `len` bytes.
    End { text: usize, len: usize },
    /// Its end has not arrived.
    Incomplete,
    /// A byte that cannot be in it sits at `body[at]`.
    Broken { at: usize },
}

/// The end of a string sequence's body, after its two-byte introducer.
fn string_end(body: &[u8]) -> StringEnd {
    for (i, &b) in body.iter().enumerate() {
        match b {
            0x07 => {
                return StringEnd::End {
                    text: i,
                    len: 2 + i + 1,
                };
            }
            ESC => {
                return match body.get(i + 1) {
                    None => StringEnd::Incomplete,
                    Some(b'\\') => StringEnd::End {
                        text: i,
                        len: 2 + i + 2,
                    },
                    Some(_) => StringEnd::Broken { at: i },
                };
            }
            0x00..=0x1F => return StringEnd::Broken { at: i },
            _ => {}
        }
    }
    StringEnd::Incomplete
}

/// A string sequence in `buf` read to its end, `read` turning its text into
/// an event; `None` from `read` passes the sequence on as unknown.
fn parse_string(buf: &[u8], read: impl FnOnce(&[u8]) -> Option<Event>) -> Parsed {
    // Read no further than the longest sequence, so that where one is given
    // up does not depend on how much has arrived.
    let buf = &buf[..buf.len().min(MAX_STRING + 1)];
    match string_end(&buf[2..]) {
        StringEnd::End { text, len } => match read(&buf[2..2 + text]) {
            Some(event) => Parsed::Event(event, len),
            None => Parsed::unknown(&buf[..len]),
        },
        StringEnd::Incomplete if buf.len() > MAX_STRING => Parsed::unknown(buf),
        StringEnd::Incomplete => Parsed::Incomplete,
        StringEnd::Broken { at } => Parsed::unknown(&buf[..2 + at]),
    }
}

/// `ESC ]` and a digit: an OSC answer, or Alt+] and typing after it.
pub(super) fn parse_osc(buf: &[u8]) -> Parsed {
    let body = &buf[2..];
    let digits = body.iter().take_while(|b| b.is_ascii_digit()).count();
    match body.get(digits) {
        None if digits <= MAX_OSC_DIGITS => return Parsed::Incomplete,
        Some(b';') if digits <= MAX_OSC_DIGITS => {}
        _ => return parse_alt(buf, false),
    }
    parse_string(buf, |text| {
        let color = terminal_color(std::str::from_utf8(text).ok()?)?;
        Some(Event::Report(Report::TerminalColor(color)))
    })
}

/// A colour answer's text (`10;rgb:…`, `4;n;rgb:…`) as a [`TerminalColor`].
fn terminal_color(text: &str) -> Option<TerminalColor> {
    let (ps, rest) = text.split_once(';')?;
    let (slot, spec) = match ps {
        "10" => (ColorSlot::Foreground, rest),
        "11" => (ColorSlot::Background, rest),
        "12" => (ColorSlot::Cursor, rest),
        "4" => {
            let (n, spec) = rest.split_once(';')?;
            (ColorSlot::Palette(n.parse().ok()?), spec)
        }
        _ => return None,
    };
    let mut channels = spec.strip_prefix("rgb:")?.split('/');
    let mut channel = || {
        let digits = channels.next()?;
        if !(1..=4).contains(&digits.len()) {
            return None;
        }
        let value = u32::from_str_radix(digits, 16).ok()?;
        // X11 scales n digits to 16 bits: v * 0xffff / (16^n - 1).
        let max = (1u32 << (4 * digits.len())) - 1;
        u16::try_from(value * 0xffff / max).ok()
    };
    let rgb = (channel()?, channel()?, channel()?);
    if channels.next().is_some() {
        return None;
    }
    Some(TerminalColor { slot, rgb })
}

/// `ESC P >`: XTVERSION's answer, `DCS > | text ST`, or Alt+P and typing.
pub(super) fn parse_dcs(buf: &[u8]) -> Parsed {
    match buf.get(3) {
        None => return Parsed::Incomplete,
        Some(b'|') => {}
        Some(_) => return parse_alt(buf, false),
    }
    parse_string(buf, |text| {
        let version = std::str::from_utf8(&text[2..]).ok()?;
        Some(Event::Report(Report::TerminalVersion(version.to_owned())))
    })
}
