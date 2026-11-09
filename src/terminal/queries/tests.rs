//! Each query's bytes, and that the answer to it decodes as its report.

use super::*;
use crate::event::{Decoder, Event, KeyboardEnhancementFlags, ModeSetting, Report, TerminalColor};
use crate::test_bytes::bytes;

fn report(answer: &[u8]) -> Report {
    let mut decoder = Decoder::new();
    decoder.feed(answer);
    match decoder.next_event() {
        Some(Event::Report(report)) => report,
        other => panic!("{other:?}"),
    }
}

#[test]
fn each_query_writes_its_question() {
    assert_eq!(bytes(RequestKeyboardEnhancementFlags), "\x1B[?u");
    assert_eq!(bytes(RequestPrimaryDeviceAttributes), "\x1B[c");
    assert_eq!(bytes(RequestSecondaryDeviceAttributes), "\x1B[>c");
    assert_eq!(bytes(RequestTerminalVersion), "\x1B[>q");
    let sync = RequestMode {
        mode: 2026,
        private: true,
    };
    assert_eq!(bytes(sync), "\x1B[?2026$p");
    let insert = RequestMode {
        mode: 4,
        private: false,
    };
    assert_eq!(bytes(insert), "\x1B[4$p");
    assert_eq!(
        bytes(RequestTerminalColor(ColorSlot::Foreground)),
        "\x1B]10;?\x1B\\"
    );
    assert_eq!(
        bytes(RequestTerminalColor(ColorSlot::Background)),
        "\x1B]11;?\x1B\\"
    );
    assert_eq!(
        bytes(RequestTerminalColor(ColorSlot::Cursor)),
        "\x1B]12;?\x1B\\"
    );
    assert_eq!(
        bytes(RequestTerminalColor(ColorSlot::Palette(201))),
        "\x1B]4;201;?\x1B\\"
    );
    assert_eq!(bytes(RequestTextAreaPixels), "\x1B[14t");
}

/// Each answer as a terminal sends it (kitty's for the kitty flags and
/// XTVERSION, xterm's for the rest), decoded as the report its query names.
#[test]
fn each_answer_decodes_as_its_report() {
    assert_eq!(
        report(b"\x1B[?1u"),
        Report::KeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    assert_eq!(
        report(b"\x1B[?62;22c"),
        Report::PrimaryDeviceAttributes(vec![62, 22])
    );
    assert_eq!(
        report(b"\x1B[>41;390;0c"),
        Report::SecondaryDeviceAttributes(vec![41, 390, 0])
    );
    assert_eq!(
        report(b"\x1BP>|kitty(0.39.1)\x1B\\"),
        Report::TerminalVersion("kitty(0.39.1)".into())
    );
    assert_eq!(
        report(b"\x1B[?2026;2$y"),
        Report::Mode {
            mode: 2026,
            private: true,
            setting: ModeSetting::Reset,
        }
    );
    assert_eq!(
        report(b"\x1B]4;201;rgb:ffff/0000/ffff\x1B\\"),
        Report::TerminalColor(TerminalColor {
            slot: ColorSlot::Palette(201),
            rgb: (0xffff, 0, 0xffff),
        })
    );
    assert_eq!(
        report(b"\x1B[4;600;800t"),
        Report::TextAreaPixels {
            width: 800,
            height: 600,
        }
    );
}
