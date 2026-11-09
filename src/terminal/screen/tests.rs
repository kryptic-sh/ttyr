//! Each screen command's bytes, as crossterm 0.29 writes them.

use super::*;
use crate::test_bytes::bytes;

#[test]
fn clears() {
    let clears = [
        (ClearType::All, "\x1B[2J"),
        (ClearType::Purge, "\x1B[3J"),
        (ClearType::FromCursorDown, "\x1B[J"),
        (ClearType::FromCursorUp, "\x1B[1J"),
        (ClearType::CurrentLine, "\x1B[2K"),
        (ClearType::UntilNewLine, "\x1B[K"),
        (ClearType::FromLineStart, "\x1B[1K"),
    ];
    for (clear, expected) in clears {
        assert_eq!(bytes(Clear(clear)), expected);
    }
}

#[test]
fn scrolling() {
    assert_eq!(bytes(ScrollUp(3)), "\x1B[3S");
    assert_eq!(bytes(ScrollDown(3)), "\x1B[3T");
    assert_eq!(bytes(ScrollUp(0)), "");
    assert_eq!(bytes(ScrollDown(0)), "");
    assert_eq!(bytes(SetScrollRegion { top: 0, bottom: 23 }), "\x1B[1;24r");
    assert_eq!(bytes(ResetScrollRegion), "\x1B[r");
}

#[test]
fn line_wrap() {
    assert_eq!(bytes(DisableLineWrap), "\x1B[?7l");
    assert_eq!(bytes(EnableLineWrap), "\x1B[?7h");
}

#[test]
fn title() {
    assert_eq!(bytes(SetTitle("ttyr — 1")), "\x1B]0;ttyr — 1\x07");
}
