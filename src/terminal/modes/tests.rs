//! Each mode's bytes, as xterm's ctlseqs and kitty's keyboard protocol
//! document them.

use super::*;

fn bytes(command: impl Command) -> String {
    let mut out = String::new();
    command.write_ansi(&mut out).unwrap();
    out
}

#[test]
fn each_mode_writes_its_sequence() {
    assert_eq!(bytes(EnterAlternateScreen), "\x1B[?1049h");
    assert_eq!(bytes(LeaveAlternateScreen), "\x1B[?1049l");
    assert_eq!(
        bytes(EnableMouseCapture),
        "\x1B[?1000h\x1B[?1002h\x1B[?1003h\x1B[?1006h"
    );
    assert_eq!(
        bytes(DisableMouseCapture),
        "\x1B[?1006l\x1B[?1003l\x1B[?1002l\x1B[?1000l"
    );
    assert_eq!(bytes(EnableBracketedPaste), "\x1B[?2004h");
    assert_eq!(bytes(DisableBracketedPaste), "\x1B[?2004l");
    assert_eq!(bytes(EnableFocusChange), "\x1B[?1004h");
    assert_eq!(bytes(DisableFocusChange), "\x1B[?1004l");
    assert_eq!(bytes(EnableModifyOtherKeys), "\x1B[>4;2m");
    assert_eq!(bytes(DisableModifyOtherKeys), "\x1B[>4m");
    assert_eq!(bytes(EnableColorSchemeReports), "\x1B[?2031h");
    assert_eq!(bytes(DisableColorSchemeReports), "\x1B[?2031l");
    assert_eq!(bytes(RequestColorScheme), "\x1B[?996n");
    assert_eq!(bytes(BeginSynchronizedUpdate), "\x1B[?2026h");
    assert_eq!(bytes(EndSynchronizedUpdate), "\x1B[?2026l");
}

#[test]
fn keyboard_flags_push_set_and_pop() {
    let flags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES;
    assert_eq!(bytes(PushKeyboardEnhancementFlags(flags)), "\x1B[>11u");
    assert_eq!(bytes(SetKeyboardEnhancementFlags(flags)), "\x1B[=11;1u");
    assert_eq!(bytes(PopKeyboardEnhancementFlags), "\x1B[<1u");
}

#[test]
fn mouse_tracking_turns_on_one_level() {
    let normal = SetMouseTracking {
        tracking: MouseTracking::Normal,
        sgr: true,
    };
    assert_eq!(bytes(normal), "\x1B[?1000h\x1B[?1006h");
    let any = SetMouseTracking {
        tracking: MouseTracking::Any,
        sgr: false,
    };
    assert_eq!(bytes(any), "\x1B[?1003h");
    let button = SetMouseTracking {
        tracking: MouseTracking::Button,
        sgr: false,
    };
    assert_eq!(bytes(button), "\x1B[?1002h");
}
