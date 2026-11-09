//! Each cursor command's bytes, as crossterm 0.29 writes them.

use super::*;
use crate::test_bytes::bytes;

#[test]
fn moves_are_one_based_on_the_wire() {
    assert_eq!(bytes(MoveTo(0, 0)), "\x1B[1;1H");
    assert_eq!(bytes(MoveTo(9, 4)), "\x1B[5;10H");
    assert_eq!(bytes(MoveTo(u16::MAX, u16::MAX)), "\x1B[65536;65536H");
    assert_eq!(bytes(MoveToColumn(3)), "\x1B[4G");
    assert_eq!(bytes(MoveToRow(3)), "\x1B[4d");
    assert_eq!(bytes(MoveUp(2)), "\x1B[2A");
    assert_eq!(bytes(MoveDown(2)), "\x1B[2B");
    assert_eq!(bytes(MoveRight(2)), "\x1B[2C");
    assert_eq!(bytes(MoveLeft(2)), "\x1B[2D");
    assert_eq!(bytes(MoveToNextLine(2)), "\x1B[2E");
    assert_eq!(bytes(MoveToPreviousLine(2)), "\x1B[2F");
}

#[test]
fn the_fixed_commands() {
    assert_eq!(bytes(SavePosition), "\x1B7");
    assert_eq!(bytes(RestorePosition), "\x1B8");
    assert_eq!(bytes(Hide), "\x1B[?25l");
    assert_eq!(bytes(Show), "\x1B[?25h");
    assert_eq!(bytes(EnableBlinking), "\x1B[?12h");
    assert_eq!(bytes(DisableBlinking), "\x1B[?12l");
    assert_eq!(bytes(RequestCursorPosition), "\x1B[6n");
    assert_eq!(bytes(ResetCursorColor), "\x1B]112\x1B\\");
}

#[test]
fn styles_and_colour() {
    let styles = [
        (SetCursorStyle::DefaultUserShape, "\x1B[0 q"),
        (SetCursorStyle::BlinkingBlock, "\x1B[1 q"),
        (SetCursorStyle::SteadyBlock, "\x1B[2 q"),
        (SetCursorStyle::BlinkingUnderScore, "\x1B[3 q"),
        (SetCursorStyle::SteadyUnderScore, "\x1B[4 q"),
        (SetCursorStyle::BlinkingBar, "\x1B[5 q"),
        (SetCursorStyle::SteadyBar, "\x1B[6 q"),
    ];
    for (style, expected) in styles {
        assert_eq!(bytes(style), expected);
    }
    let color = SetCursorColor {
        r: 0xff,
        g: 0x0a,
        b: 0,
    };
    assert_eq!(bytes(color), "\x1B]12;rgb:ff/0a/00\x1B\\");
}
