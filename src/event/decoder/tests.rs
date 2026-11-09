use super::Decoder;
use crate::event::{
    ColorScheme, ColorSlot, Event, KeyCode, KeyEncoding, KeyEvent, KeyEventKind, KeyEventState,
    KeyModifiers, KeyboardEnhancementFlags, MediaKeyCode, ModeSetting, ModifierKeyCode,
    MouseButton, MouseEvent, MouseEventKind, Report, TerminalColor,
};

const NONE: KeyModifiers = KeyModifiers::NONE;
const SHIFT: KeyModifiers = KeyModifiers::SHIFT;
const CTRL: KeyModifiers = KeyModifiers::CONTROL;
const ALT: KeyModifiers = KeyModifiers::ALT;

/// Every event in `bytes`, fed at once, taking them as all there is.
fn decode(bytes: &[u8]) -> Vec<Event> {
    let mut decoder = Decoder::new();
    decoder.feed(bytes);
    drain(&mut decoder)
}

/// Every event the decoder has, then everything it holds, flushed.
fn drain(decoder: &mut Decoder) -> Vec<Event> {
    let mut events = Vec::new();
    while let Some(event) = decoder.next_event() {
        events.push(event);
    }
    while let Some(event) = decoder.flush_event() {
        events.push(event);
    }
    events
}

/// The one event in `bytes`.
fn one(bytes: &[u8]) -> Event {
    match decode(bytes).as_slice() {
        [event] => event.clone(),
        events => panic!("{bytes:?}: {events:?}"),
    }
}

/// The one key event in `bytes`.
fn key(bytes: &[u8]) -> KeyEvent {
    match one(bytes) {
        Event::Key(key) => key,
        event => panic!("{bytes:?}: {event:?}"),
    }
}

fn legacy(key: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(key, modifiers).with_encoding(KeyEncoding::Legacy)
}

fn kitty(key: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(key, modifiers)
}

fn report(bytes: &[u8]) -> Report {
    match one(bytes) {
        Event::Report(report) => report,
        event => panic!("{bytes:?}: {event:?}"),
    }
}

// ── Legacy ──────────────────────────────────────────────────────────────────

#[test]
fn legacy_bytes() {
    let c = KeyCode::Char;
    for (bytes, expected) in [
        (&b"a"[..], legacy(c('a'), NONE)),
        (b"1", legacy(c('1'), NONE)),
        (b"!", legacy(c('!'), NONE)),
        (b"A", legacy(c('a'), SHIFT).with_shifted(Some('A'))),
        ("é".as_bytes(), legacy(c('é'), NONE)),
        (
            "É".as_bytes(),
            legacy(c('é'), SHIFT).with_shifted(Some('É')),
        ),
        ("😀".as_bytes(), legacy(c('😀'), NONE)),
        (b"\r", legacy(KeyCode::Enter, NONE)),
        (b"\t", legacy(KeyCode::Tab, NONE)),
        (b"\x7F", legacy(KeyCode::Backspace, NONE)),
        (b"\x00", legacy(c(' '), CTRL)),
        (b"\x01", legacy(c('a'), CTRL)),
        (b"\x08", legacy(c('h'), CTRL)),
        (b"\n", legacy(c('j'), CTRL)),
        (b"\x1A", legacy(c('z'), CTRL)),
        (b"\x1C", legacy(c('4'), CTRL)),
        (b"\x1F", legacy(c('7'), CTRL)),
        (b"\x1B", legacy(KeyCode::Esc, NONE)),
        (b"\x1Ba", legacy(c('a'), ALT)),
        (
            b"\x1BA",
            legacy(c('a'), ALT | SHIFT).with_shifted(Some('A')),
        ),
        (b"\x1B\x01", legacy(c('a'), ALT | CTRL)),
        (b"\x1B\x7F", legacy(KeyCode::Backspace, ALT)),
        (b"\x1B[", legacy(c('['), ALT)),
        (
            b"\x1BO",
            legacy(c('o'), ALT | SHIFT).with_shifted(Some('O')),
        ),
        (b"\x1B]", legacy(c(']'), ALT)),
        (
            b"\x1BP",
            legacy(c('p'), ALT | SHIFT).with_shifted(Some('P')),
        ),
    ] {
        assert_eq!(key(bytes), expected, "{bytes:?}");
    }
}

#[test]
fn legacy_cursor_and_function_keys() {
    let ctrl_shift = CTRL | SHIFT;
    for (bytes, expected) in [
        (&b"\x1B[A"[..], legacy(KeyCode::Up, NONE)),
        (b"\x1B[D", legacy(KeyCode::Left, NONE)),
        (b"\x1B[H", legacy(KeyCode::Home, NONE)),
        (b"\x1B[F", legacy(KeyCode::End, NONE)),
        (b"\x1B[E", legacy(KeyCode::KeypadBegin, NONE)),
        (b"\x1B[1;5A", legacy(KeyCode::Up, CTRL)),
        (b"\x1B[1;2P", legacy(KeyCode::F(1), SHIFT)),
        (b"\x1B[Z", legacy(KeyCode::BackTab, SHIFT)),
        (b"\x1B[2~", legacy(KeyCode::Insert, NONE)),
        (b"\x1B[3;5~", legacy(KeyCode::Delete, CTRL)),
        (b"\x1B[5~", legacy(KeyCode::PageUp, NONE)),
        (b"\x1B[6~", legacy(KeyCode::PageDown, NONE)),
        (b"\x1B[1~", legacy(KeyCode::Home, NONE)),
        (b"\x1B[4~", legacy(KeyCode::End, NONE)),
        (b"\x1B[15;2~", legacy(KeyCode::F(5), SHIFT)),
        (b"\x1B[24~", legacy(KeyCode::F(12), NONE)),
        (b"\x1B[34~", legacy(KeyCode::F(20), NONE)),
        (b"\x1BOA", legacy(KeyCode::Up, NONE)),
        (b"\x1BOP", legacy(KeyCode::F(1), NONE)),
        (b"\x1BOS", legacy(KeyCode::F(4), NONE)),
        (b"\x1BO5P", legacy(KeyCode::F(1), CTRL)),
        // The keypad in application mode.
        (
            b"\x1BOp",
            legacy(KeyCode::Char('0'), NONE).with_state(KeyEventState::KEYPAD),
        ),
        (
            b"\x1BOy",
            legacy(KeyCode::Char('9'), NONE).with_state(KeyEventState::KEYPAD),
        ),
        (
            b"\x1BOM",
            legacy(KeyCode::Enter, NONE).with_state(KeyEventState::KEYPAD),
        ),
        (
            b"\x1BOk",
            legacy(KeyCode::Char('+'), NONE).with_state(KeyEventState::KEYPAD),
        ),
        // The Linux console's F1 to F5.
        (b"\x1B[[A", legacy(KeyCode::F(1), NONE)),
        (b"\x1B[[E", legacy(KeyCode::F(5), NONE)),
        // rxvt: Ctrl+arrows, Shift+arrows, Ctrl and Ctrl+Shift editing keys.
        (b"\x1BOa", legacy(KeyCode::Up, CTRL)),
        (b"\x1B[d", legacy(KeyCode::Left, SHIFT)),
        (b"\x1B[2^", legacy(KeyCode::Insert, CTRL)),
        (b"\x1B[3@", legacy(KeyCode::Delete, ctrl_shift)),
        (b"\x1B[2$", legacy(KeyCode::Insert, SHIFT)),
        // Alt with any of them.
        (b"\x1B\x1B[A", legacy(KeyCode::Up, ALT)),
        (b"\x1B\x1BOP", legacy(KeyCode::F(1), ALT)),
    ] {
        assert_eq!(key(bytes), expected, "{bytes:?}");
    }
}

#[test]
fn legacy_modifier_bit_8_is_super_as_kitty_sends_it() {
    assert_eq!(key(b"\x1B[1;9A"), legacy(KeyCode::Up, KeyModifiers::SUPER));
}

// ── tmxr's crossterm fork, each fix ─────────────────────────────────────────

#[test]
fn fork_esc_esc_is_alt_esc() {
    // 05e475c: ESC ESC at the end of the input is Alt+Esc.
    let mut decoder = Decoder::new();
    decoder.feed(b"\x1B\x1B");
    assert_eq!(decoder.next_event(), None);
    assert_eq!(drain(&mut decoder), [Event::Key(legacy(KeyCode::Esc, ALT))]);
    assert!(!decoder.has_pending());
}

#[test]
fn fork_unknown_private_csi_does_not_swallow_input() {
    // 1e98694 (crossterm#1106): a finished report ttyr does not know is
    // passed on, and what follows it is read.
    assert_eq!(
        decode(b"\x1B[?1;2;3zab"),
        [
            Event::Unknown(b"\x1B[?1;2;3z".to_vec()),
            Event::Key(legacy(KeyCode::Char('a'), NONE)),
            Event::Key(legacy(KeyCode::Char('b'), NONE)),
        ]
    );
}

#[test]
fn fork_color_scheme() {
    // ef87b58.
    assert_eq!(
        report(b"\x1B[?997;1n"),
        Report::ColorScheme(ColorScheme::Dark)
    );
    assert_eq!(
        report(b"\x1B[?997;2n"),
        Report::ColorScheme(ColorScheme::Light)
    );
    assert_eq!(
        one(b"\x1B[?997;3n"),
        Event::Unknown(b"\x1B[?997;3n".to_vec())
    );
}

#[test]
fn fork_osc_terminal_color() {
    // 6957a38, its cases.
    let color = |slot, rgb| Report::TerminalColor(TerminalColor { slot, rgb });
    assert_eq!(
        report(b"\x1B]11;rgb:1e1e/1e1e/2e2e\x1B\\"),
        color(ColorSlot::Background, (0x1e1e, 0x1e1e, 0x2e2e))
    );
    assert_eq!(
        report(b"\x1B]10;rgb:ff/80/00\x07"),
        color(ColorSlot::Foreground, (0xffff, 0x8080, 0))
    );
    assert_eq!(
        report(b"\x1B]12;rgb:f/0/0\x07"),
        color(ColorSlot::Cursor, (0xffff, 0, 0))
    );
    assert_eq!(
        report(b"\x1B]4;1;rgb:cdcd/0000/0000\x1B\\"),
        color(ColorSlot::Palette(1), (0xcdcd, 0, 0))
    );
    // Every part of an answer waits for the rest, flushed or not.
    let answer = b"\x1B]4;255;rgb:cdcd/0000/0000\x1B\\";
    for end in 3..answer.len() {
        let mut decoder = Decoder::new();
        decoder.feed(&answer[..end]);
        assert_eq!(decoder.flush_event(), None, "{end}");
        decoder.feed(&answer[end..]);
        assert_eq!(
            decoder.next_event(),
            Some(Event::Report(color(
                ColorSlot::Palette(255),
                (0xcdcd, 0, 0)
            ))),
            "{end}"
        );
    }
    // Not colour answers: passed on whole.
    for bytes in [
        &b"\x1B]52;c;aGk=\x07"[..],
        b"\x1B]11;black\x07",
        b"\x1B]11;rgb:1/2/3/4\x07",
        b"\x1B]4;256;rgb:0/0/0\x07",
    ] {
        assert_eq!(one(bytes), Event::Unknown(bytes.to_vec()), "{bytes:?}");
    }
}

#[test]
fn fork_esc_bracket_is_alt_bracket() {
    let alt_bracket = Event::Key(legacy(KeyCode::Char(']'), ALT));
    assert_eq!(decode(b"\x1B]"), std::slice::from_ref(&alt_bracket));
    let mut decoder = Decoder::new();
    decoder.feed(b"\x1B]");
    assert_eq!(decoder.next_event(), None);
    // Unlike the fork, keys after it are kept.
    assert_eq!(
        decode(b"\x1B]a"),
        [
            alt_bracket.clone(),
            Event::Key(legacy(KeyCode::Char('a'), NONE))
        ]
    );
    assert_eq!(
        decode(b"\x1B]1x"),
        [
            alt_bracket,
            Event::Key(legacy(KeyCode::Char('1'), NONE)),
            Event::Key(legacy(KeyCode::Char('x'), NONE)),
        ]
    );
}

#[test]
fn fork_burst_longer_than_a_read_arrives_whole() {
    // 08697c0, 03b39a0: at the decoder, a burst in any number of reads is
    // every event once, handed back as each read arrives.
    let burst = b"\x1B[1;5A".repeat(2000);
    let mut decoder = Decoder::new();
    let mut events = 0;
    for chunk in burst.chunks(1024) {
        decoder.feed(chunk);
        while let Some(event) = decoder.next_event() {
            assert_eq!(event, Event::Key(legacy(KeyCode::Up, CTRL)));
            events += 1;
        }
    }
    assert_eq!(events, 2000);
    assert!(!decoder.has_pending());
}

// ── kitty keyboard protocol ─────────────────────────────────────────────────
// Sequences from the spec (sw.kovidgoyal.net/kitty/keyboard-protocol), by
// section.

#[test]
fn kitty_disambiguate_escape_codes() {
    // "Disambiguate escape codes": Esc, and Ctrl+[ apart from it.
    assert_eq!(key(b"\x1B[27u"), kitty(KeyCode::Esc, NONE));
    assert_eq!(key(b"\x1B[91;5u"), kitty(KeyCode::Char('['), CTRL));
    assert_eq!(key(b"\x1B[105;5u"), kitty(KeyCode::Char('i'), CTRL));
    assert_eq!(key(b"\x1B[97;3u"), kitty(KeyCode::Char('a'), ALT));
    assert_eq!(key(b"\x1B[97;4u"), kitty(KeyCode::Char('a'), ALT | SHIFT));
}

#[test]
fn kitty_modifiers_and_lock_keys() {
    // "Modifiers": the lock keys go to the state, never the modifiers, so a
    // keymap's `modifiers == CONTROL` holds with Caps Lock on.
    let event = key(b"\x1B[97;69u");
    assert_eq!(event.modifiers, CTRL);
    assert_eq!(event.state, KeyEventState::CAPS_LOCK);
    let event = key(b"\x1B[97;133u");
    assert_eq!(event.modifiers, CTRL);
    assert_eq!(event.state, KeyEventState::NUM_LOCK);
    let all = KeyModifiers::all() - KeyModifiers::NONE;
    let event = key(b"\x1B[97;256u");
    assert_eq!(event.modifiers, all);
    assert_eq!(
        event.state,
        KeyEventState::CAPS_LOCK | KeyEventState::NUM_LOCK
    );
    let event = key(b"\x1B[97;9u");
    assert_eq!(event.modifiers, KeyModifiers::SUPER);
    let event = key(b"\x1B[97;17u");
    assert_eq!(event.modifiers, KeyModifiers::HYPER);
    let event = key(b"\x1B[97;33u");
    assert_eq!(event.modifiers, KeyModifiers::META);
}

#[test]
fn kitty_event_types() {
    // "Event types": `mods:kind`, the modifiers' default 1 when only the
    // kind is wanted.
    assert_eq!(key(b"\x1B[97;1:1u").kind, KeyEventKind::Press);
    assert_eq!(key(b"\x1B[97;1:2u").kind, KeyEventKind::Repeat);
    assert_eq!(key(b"\x1B[97;1:3u").kind, KeyEventKind::Release);
    assert_eq!(
        key(b"\x1B[97;:3u"),
        kitty(KeyCode::Char('a'), NONE).with_kind(KeyEventKind::Release)
    );
    // Legacy-form keys carry the kind too.
    assert_eq!(
        key(b"\x1B[1;1:3A"),
        legacy(KeyCode::Up, NONE).with_kind(KeyEventKind::Release)
    );
    assert_eq!(
        key(b"\x1B[3;5:2~"),
        legacy(KeyCode::Delete, CTRL).with_kind(KeyEventKind::Repeat)
    );
    // A kind the protocol does not define is not guessed at.
    assert_eq!(
        one(b"\x1B[97;1:4u"),
        Event::Unknown(b"\x1B[97;1:4u".to_vec())
    );
}

#[test]
fn kitty_alternate_keys() {
    // "Report alternate keys": `key:shifted:base`, either part empty.
    let shift_a = key(b"\x1B[97:65;2u");
    assert_eq!(
        shift_a,
        kitty(KeyCode::Char('a'), SHIFT).with_shifted(Some('A'))
    );
    // Ctrl+С on a Cyrillic layout, the base layout's key c.
    let cyrillic = key(b"\x1B[1089::99;5u");
    assert_eq!(
        cyrillic,
        kitty(KeyCode::Char('с'), CTRL).with_base_layout(Some(KeyCode::Char('c')))
    );
    // The tmxr e2e harness's Ctrl+Shift+h.
    let event = key(b"\x1B[104:72;6u");
    assert_eq!(
        event,
        kitty(KeyCode::Char('h'), CTRL | SHIFT).with_shifted(Some('H'))
    );
    assert_eq!(event.logical_char(), Some('H'));
    assert_eq!(event.logical_modifiers(), CTRL);
    // Ctrl+Shift+é: the shifted key reported, not folded.
    let event = key("\x1B[233:201;6u".as_bytes());
    assert_eq!(event.key, KeyCode::Char('é'));
    assert_eq!(event.logical_char(), Some('É'));
}

#[test]
fn kitty_associated_text() {
    // "Report associated text": the text's code points, `:`-separated.
    let event = key(b"\x1B[97;2;65u");
    assert_eq!(
        event,
        kitty(KeyCode::Char('a'), SHIFT).with_text(Some("A".into()))
    );
    let event = key(b"\x1B[97;;97u");
    assert_eq!(event.text.as_deref(), Some("a"));
    let event = key(b"\x1B[0;;104:105u");
    assert_eq!(
        event,
        kitty(KeyCode::Null, NONE).with_text(Some("hi".into()))
    );
}

#[test]
fn kitty_functional_keys() {
    // "Functional key definitions".
    let keypad = KeyEventState::KEYPAD;
    for (bytes, expected) in [
        (&b"\x1B[13u"[..], kitty(KeyCode::Enter, NONE)),
        (b"\x1B[9u", kitty(KeyCode::Tab, NONE)),
        (b"\x1B[9;2u", kitty(KeyCode::BackTab, SHIFT)),
        (b"\x1B[127u", kitty(KeyCode::Backspace, NONE)),
        (b"\x1B[57358u", kitty(KeyCode::CapsLock, NONE)),
        (b"\x1B[57363u", kitty(KeyCode::Menu, NONE)),
        (b"\x1B[57376u", kitty(KeyCode::F(13), NONE)),
        (b"\x1B[57398u", kitty(KeyCode::F(35), NONE)),
        (
            b"\x1B[57399u",
            kitty(KeyCode::Char('0'), NONE).with_state(keypad),
        ),
        (
            b"\x1B[57414u",
            kitty(KeyCode::Enter, NONE).with_state(keypad),
        ),
        (
            b"\x1B[57427u",
            kitty(KeyCode::KeypadBegin, NONE).with_state(keypad),
        ),
        (
            b"\x1B[57428u",
            kitty(KeyCode::Media(MediaKeyCode::Play), NONE),
        ),
        (
            b"\x1B[57440u",
            kitty(KeyCode::Media(MediaKeyCode::MuteVolume), NONE),
        ),
        // A modifier key's own press, modifiers as reported.
        (
            b"\x1B[57441;2u",
            kitty(KeyCode::Modifier(ModifierKeyCode::LeftShift), SHIFT),
        ),
        (
            b"\x1B[57454u",
            kitty(KeyCode::Modifier(ModifierKeyCode::IsoLevel5Shift), NONE),
        ),
        // F3 and the editing keys keep their legacy forms.
        (b"\x1B[13~", legacy(KeyCode::F(3), NONE)),
    ] {
        assert_eq!(key(bytes), expected, "{bytes:?}");
    }
}

#[test]
fn kitty_flags_report() {
    assert_eq!(
        report(b"\x1B[?15u"),
        Report::KeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    );
    assert_eq!(
        report(b"\x1B[?0u"),
        Report::KeyboardEnhancementFlags(KeyboardEnhancementFlags::empty())
    );
}

// ── xterm modifyOtherKeys ───────────────────────────────────────────────────

#[test]
fn xterm_modify_other_keys() {
    let xterm = |key, mods| KeyEvent::new(key, mods).with_encoding(KeyEncoding::Xterm);
    assert_eq!(key(b"\x1B[27;5;97~"), xterm(KeyCode::Char('a'), CTRL));
    assert_eq!(
        key(b"\x1B[27;6;65~"),
        xterm(KeyCode::Char('a'), CTRL | SHIFT).with_shifted(Some('A'))
    );
    assert_eq!(key(b"\x1B[27;5;13~"), xterm(KeyCode::Enter, CTRL));
    assert_eq!(key(b"\x1B[27;5;9~"), xterm(KeyCode::Tab, CTRL));
    assert_eq!(key(b"\x1B[27;2;9~"), xterm(KeyCode::BackTab, SHIFT));
    // xterm's bit 8 is Meta.
    assert_eq!(
        key(b"\x1B[27;9;97~"),
        xterm(KeyCode::Char('a'), KeyModifiers::META)
    );
    let event = key(b"\x1B[27;4;65~");
    assert_eq!(
        (event.logical_char(), event.logical_modifiers()),
        (Some('A'), ALT)
    );
}

// ── Mouse, focus, paste ─────────────────────────────────────────────────────

fn mouse(kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers,
    })
}

#[test]
fn mouse_sgr() {
    use MouseButton::{Back, Forward, Left, Middle, Right};
    use MouseEventKind::{Down, Drag, Moved, ScrollDown, ScrollLeft, ScrollRight, ScrollUp, Up};
    for (bytes, expected) in [
        (&b"\x1B[<0;20;10M"[..], mouse(Down(Left), 19, 9, NONE)),
        (b"\x1B[<0;20;10m", mouse(Up(Left), 19, 9, NONE)),
        (b"\x1B[<1;1;1M", mouse(Down(Middle), 0, 0, NONE)),
        (b"\x1B[<2;1;1M", mouse(Down(Right), 0, 0, NONE)),
        (b"\x1B[<32;5;5M", mouse(Drag(Left), 4, 4, NONE)),
        (b"\x1B[<35;5;5M", mouse(Moved, 4, 4, NONE)),
        (b"\x1B[<64;1;1M", mouse(ScrollUp, 0, 0, NONE)),
        (b"\x1B[<65;1;1M", mouse(ScrollDown, 0, 0, NONE)),
        (b"\x1B[<66;1;1M", mouse(ScrollLeft, 0, 0, NONE)),
        (b"\x1B[<67;1;1M", mouse(ScrollRight, 0, 0, NONE)),
        (b"\x1B[<128;1;1M", mouse(Down(Back), 0, 0, NONE)),
        (b"\x1B[<129;1;1m", mouse(Up(Forward), 0, 0, NONE)),
        (b"\x1B[<4;1;1M", mouse(Down(Left), 0, 0, SHIFT)),
        (b"\x1B[<8;1;1M", mouse(Down(Left), 0, 0, ALT)),
        (b"\x1B[<16;1;1M", mouse(Down(Left), 0, 0, CTRL)),
        // SGR-pixels (1016): the same form, in pixels.
        (b"\x1B[<0;1200;800M", mouse(Down(Left), 1199, 799, NONE)),
    ] {
        assert_eq!(one(bytes), expected, "{bytes:?}");
    }
}

#[test]
fn mouse_x10_and_rxvt() {
    use MouseButton::Left;
    use MouseEventKind::{Down, Up};
    assert_eq!(one(b"\x1B[M\x20\x21\x21"), mouse(Down(Left), 0, 0, NONE));
    assert_eq!(one(b"\x1B[M\x23\x30\x40"), mouse(Up(Left), 15, 31, NONE));
    assert_eq!(one(b"\x1B[32;30;40M"), mouse(Down(Left), 29, 39, NONE));
}

#[test]
fn focus_and_paste() {
    assert_eq!(one(b"\x1B[I"), Event::FocusGained);
    assert_eq!(one(b"\x1B[O"), Event::FocusLost);
    assert_eq!(
        one(b"\x1B[200~hi \x1B[A\r\n\x1B[201~"),
        Event::Paste("hi \x1B[A\r\n".into())
    );
    assert_eq!(one(b"\x1B[200~\x1B[201~"), Event::Paste(String::new()));
}

#[test]
fn paste_across_reads_waits_for_its_end() {
    // Flushed anywhere after its start marker, the paste is held whole.
    let paste = b"\x1B[200~one\x1B[20two\x1B[201~x";
    for split in b"\x1B[200~".len()..paste.len() {
        let mut decoder = Decoder::new();
        decoder.feed(&paste[..split]);
        let mut events = Vec::new();
        while let Some(event) = decoder.flush_event() {
            events.push(event);
        }
        decoder.feed(&paste[split..]);
        events.extend(drain(&mut decoder));
        assert_eq!(
            events,
            [
                Event::Paste("one\x1B[20two".into()),
                Event::Key(legacy(KeyCode::Char('x'), NONE))
            ],
            "{split}"
        );
    }
}

// ── Reports ─────────────────────────────────────────────────────────────────

#[test]
fn reports() {
    assert_eq!(
        report(b"\x1B[?62;22;52c"),
        Report::PrimaryDeviceAttributes(vec![62, 22, 52])
    );
    assert_eq!(
        report(b"\x1B[>1;10;0c"),
        Report::SecondaryDeviceAttributes(vec![1, 10, 0])
    );
    assert_eq!(
        report(b"\x1BP>|kitty(0.36.4)\x1B\\"),
        Report::TerminalVersion("kitty(0.36.4)".into())
    );
    assert_eq!(
        report(b"\x1B[?2026;2$y"),
        Report::Mode {
            mode: 2026,
            private: true,
            setting: ModeSetting::Reset
        }
    );
    assert_eq!(
        report(b"\x1B[4;0$y"),
        Report::Mode {
            mode: 4,
            private: false,
            setting: ModeSetting::NotRecognized
        }
    );
    assert_eq!(
        report(b"\x1B[5;10R"),
        Report::CursorPosition { row: 4, column: 9 }
    );
    assert_eq!(
        report(b"\x1B[4;600;800t"),
        Report::TextAreaPixels {
            height: 600,
            width: 800
        }
    );
}

#[test]
fn tmxr_attach_probe_answers() {
    // tmxr writes `CSI ? 2026 $ p`, `OSC 10 ; ?`, `OSC 11 ; ?` and `CSI c`
    // at attach; the answers arrive together, DA1 last.
    let answers =
        b"\x1B[?2026;2$y\x1B]10;rgb:cccc/cccc/cccc\x1B\\\x1B]11;rgb:0000/0000/0000\x07\x1B[?62;22c";
    let reports: Vec<_> = decode(answers)
        .into_iter()
        .map(|event| match event {
            Event::Report(report) => report,
            event => panic!("{event:?}"),
        })
        .collect();
    assert_eq!(
        reports,
        [
            Report::Mode {
                mode: 2026,
                private: true,
                setting: ModeSetting::Reset
            },
            Report::TerminalColor(TerminalColor {
                slot: ColorSlot::Foreground,
                rgb: (0xcccc, 0xcccc, 0xcccc)
            }),
            Report::TerminalColor(TerminalColor {
                slot: ColorSlot::Background,
                rgb: (0, 0, 0)
            }),
            Report::PrimaryDeviceAttributes(vec![62, 22]),
        ]
    );
}

// ── Malformed input ─────────────────────────────────────────────────────────

#[test]
fn invalid_bytes_are_passed_on_and_input_goes_on() {
    let a = Event::Key(legacy(KeyCode::Char('a'), NONE));
    assert_eq!(decode(b"\xFFa"), [Event::Unknown(vec![0xFF]), a.clone()]);
    assert_eq!(
        decode(b"\xC3(a"),
        [
            Event::Unknown(vec![0xC3]),
            Event::Key(legacy(KeyCode::Char('('), NONE)),
            a.clone()
        ]
    );
    // A control byte breaks a CSI sequence off; it is read on its own.
    assert_eq!(
        decode(b"\x1B[1\x01a"),
        [
            Event::Unknown(b"\x1B[1".to_vec()),
            Event::Key(legacy(KeyCode::Char('a'), CTRL)),
            a.clone()
        ]
    );
    // A CSI sequence that never ends is given up.
    let mut endless = b"\x1B[".to_vec();
    endless.resize(300, b'1');
    let events = decode(&endless);
    assert_eq!(
        events.first(),
        Some(&Event::Unknown(endless[..257].to_vec()))
    );
}

#[test]
fn incomplete_utf8_waits() {
    let mut decoder = Decoder::new();
    decoder.feed(&"é".as_bytes()[..1]);
    assert_eq!(decoder.flush_event(), None);
    decoder.feed(&"é".as_bytes()[1..]);
    assert_eq!(
        decoder.next_event(),
        Some(Event::Key(legacy(KeyCode::Char('é'), NONE)))
    );
}

#[test]
fn a_lone_esc_waits_until_flushed() {
    let mut decoder = Decoder::new();
    decoder.feed(b"\x1B");
    assert_eq!(decoder.next_event(), None);
    assert!(decoder.has_pending());
    decoder.feed(b"[A");
    assert_eq!(
        decoder.next_event(),
        Some(Event::Key(legacy(KeyCode::Up, NONE)))
    );
}

// ── Split reads ─────────────────────────────────────────────────────────────

/// One of each kind of sequence the decoder reads.
const CORPUS: &[&[u8]] = &[
    b"a",
    b"\x1B[65;30;65;1;16;1_",
    b"\x1B[0;0;55357;1;0;1_\x1B[0;0;56832;1;0;1_",
    "é😀".as_bytes(),
    b"\x1B[1;5A",
    b"\x1BOP",
    b"\x1B[15;2~",
    b"\x1B[97;1:3u",
    b"\x1B[1089::99;5u",
    b"\x1B[0;;104:105u",
    b"\x1B[27;6;65~",
    b"\x1B[<0;20;10M",
    b"\x1B[M\x20\x21\x21",
    b"\x1B[200~pasted\x1B[201~",
    b"\x1B[?2026;2$y",
    b"\x1B]11;rgb:1e1e/1e1e/2e2e\x1B\\",
    b"\x1BP>|kitty(0.36.4)\x1B\\",
    b"\x1B[?62;22c",
    b"\x1B\x1B[A",
    b"\x1B[?1;2;3z",
    b"\xFF",
];

#[test]
fn a_sequence_split_anywhere_reads_the_same() {
    let whole: Vec<u8> = CORPUS.concat();
    let expected = decode(&whole);
    assert_eq!(expected.len(), CORPUS.len() + 1, "{expected:?}"); // "é😀" is two keys.
    for split in 1..whole.len() {
        let mut decoder = Decoder::new();
        decoder.feed(&whole[..split]);
        let mut events = Vec::new();
        while let Some(event) = decoder.next_event() {
            events.push(event);
        }
        decoder.feed(&whole[split..]);
        events.extend(drain(&mut decoder));
        assert_eq!(events, expected, "split at {split}");
    }
}

#[test]
fn byte_at_a_time_reads_the_same() {
    let whole: Vec<u8> = CORPUS.concat();
    let mut decoder = Decoder::new();
    let mut events = Vec::new();
    for &b in &whole {
        decoder.feed(&[b]);
        while let Some(event) = decoder.next_event() {
            events.push(event);
        }
    }
    events.extend(drain(&mut decoder));
    assert_eq!(events, decode(&whole));
}

/// xorshift64, for repeatable noise without a dependency.
fn noise(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

#[test]
fn noise_never_panics_and_reads_the_same_in_any_chunks() {
    // Bytes weighted towards what sequences are made of.
    const ALPHABET: &[u8] = b"\x1B\x1B\x1B[[]O<?>;;:0123456789$~uMmARZy\x07\\P|a\xC3\xA9\xFF\x01";
    let mut seed = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..2000 {
        let len = usize::try_from(noise(&mut seed) % 64).unwrap();
        let input: Vec<u8> = (0..len)
            .map(|_| ALPHABET[usize::try_from(noise(&mut seed)).unwrap() % ALPHABET.len()])
            .collect();
        let expected = decode(&input);
        let mut decoder = Decoder::new();
        let mut events = Vec::new();
        let mut rest = &input[..];
        while !rest.is_empty() {
            let take = 1 + usize::try_from(noise(&mut seed)).unwrap() % rest.len();
            decoder.feed(&rest[..take]);
            rest = &rest[take..];
            while let Some(event) = decoder.next_event() {
                events.push(event);
            }
        }
        events.extend(drain(&mut decoder));
        assert_eq!(events, expected, "{input:?}");
    }
}

/// Windows Terminal's win32-input-mode: `a` pressed and released, Ctrl+Enter,
/// then a malformed record (key down 2).
#[test]
fn win32_input_mode_records() {
    let a =
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE).with_encoding(KeyEncoding::Windows);
    let ctrl_enter =
        KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL).with_encoding(KeyEncoding::Windows);
    assert_eq!(
        decode(b"\x1B[65;30;97;1;0;1_\x1B[65;30;97;0;0;1_\x1B[13;28;10;1;8;1_\x1B[65;30;97;2;0;1_"),
        [
            Event::Key(a.clone()),
            Event::Key(a.with_kind(KeyEventKind::Release)),
            Event::Key(ctrl_enter),
            Event::Unknown(b"\x1B[65;30;97;2;0;1_".to_vec()),
        ]
    );
    // A modifier key's own record is no event.
    assert_eq!(decode(b"\x1B[16;42;0;1;16;1_"), []);
}

/// A background colour answer's channels, for `spec`; `None` when the
/// answer is not read as a colour.
fn background(spec: &str) -> Option<(u16, u16, u16)> {
    match one(format!("\x1B]11;{spec}\x1B\\").as_bytes()) {
        Event::Report(Report::TerminalColor(TerminalColor {
            slot: ColorSlot::Background,
            rgb,
        })) => Some(rgb),
        _ => None,
    }
}

/// Every numeric form Xlib's `XParseColor` reads, with the values its
/// documentation gives: `rgb:` scales, `#` puts the digits in the high bits
/// (`#3a7` is `#3000a0007000`), `rgbi:` is 0 to 1.
#[test]
fn colour_answers_in_every_x11_numeric_form() {
    assert_eq!(background("rgb:f/80/abc"), Some((0xffff, 0x8080, 0xabca)));
    assert_eq!(background("#3a7"), Some((0x3000, 0xa000, 0x7000)));
    assert_eq!(background("#ff8000"), Some((0xff00, 0x8000, 0x0000)));
    assert_eq!(background("#fff000a5a"), Some((0xfff0, 0x0000, 0xa5a0)));
    assert_eq!(background("#1e1e1e1e2e2e"), Some((0x1e1e, 0x1e1e, 0x2e2e)));
    assert_eq!(background("rgbi:1/0.5/0"), Some((0xffff, 0x8000, 0)));
    assert_eq!(background("rgbi:0.0/1.0/0.25"), Some((0, 0xffff, 0x4000)));
    // Not colours: wrong lengths, out of range, too many or too few
    // channels, and names.
    for spec in [
        "#12345",
        "#",
        "#ggg",
        "rgbi:1.5/0/0",
        "rgbi:-0.1/0/0",
        "rgbi:a/b/c",
        "rgb:12345/0/0",
        "rgb:1/2",
        "rgb:1/2/3/4",
        "rgbi:0/0/0/0",
        "red",
    ] {
        assert_eq!(background(spec), None, "{spec}");
    }
}
