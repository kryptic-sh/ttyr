//! Each style command's bytes, as crossterm 0.29 writes them.

use super::*;
use crate::test_bytes::bytes;

#[test]
fn named_colours_are_palette_entries() {
    let named = [
        (Color::Black, 0),
        (Color::DarkRed, 1),
        (Color::DarkGreen, 2),
        (Color::DarkYellow, 3),
        (Color::DarkBlue, 4),
        (Color::DarkMagenta, 5),
        (Color::DarkCyan, 6),
        (Color::Grey, 7),
        (Color::DarkGrey, 8),
        (Color::Red, 9),
        (Color::Green, 10),
        (Color::Yellow, 11),
        (Color::Blue, 12),
        (Color::Magenta, 13),
        (Color::Cyan, 14),
        (Color::White, 15),
    ];
    for (color, index) in named {
        assert_eq!(
            bytes(SetForegroundColor(color)),
            format!("\x1B[38;5;{index}m")
        );
    }
}

#[test]
fn each_layer_sets_and_resets() {
    let rgb = Color::Rgb { r: 1, g: 2, b: 3 };
    assert_eq!(bytes(SetForegroundColor(rgb)), "\x1B[38;2;1;2;3m");
    assert_eq!(bytes(SetBackgroundColor(rgb)), "\x1B[48;2;1;2;3m");
    assert_eq!(bytes(SetUnderlineColor(rgb)), "\x1B[58;2;1;2;3m");
    assert_eq!(
        bytes(SetBackgroundColor(Color::AnsiValue(200))),
        "\x1B[48;5;200m"
    );
    assert_eq!(bytes(SetForegroundColor(Color::Reset)), "\x1B[39m");
    assert_eq!(bytes(SetBackgroundColor(Color::Reset)), "\x1B[49m");
    assert_eq!(bytes(SetUnderlineColor(Color::Reset)), "\x1B[59m");
    assert_eq!(bytes(ResetColor), "\x1B[0m");
}

#[test]
fn set_colors_writes_one_sequence() {
    let both = Colors::new(Color::Red, Color::Reset);
    assert_eq!(bytes(SetColors(both)), "\x1B[38;5;9;49m");
    let background = Colors {
        foreground: None,
        background: Some(Color::Blue),
    };
    assert_eq!(bytes(SetColors(background)), "\x1B[48;5;12m");
    let foreground = Colors {
        foreground: Some(Color::Blue),
        background: None,
    };
    assert_eq!(bytes(SetColors(foreground)), "\x1B[38;5;12m");
    let neither = Colors {
        foreground: None,
        background: None,
    };
    assert_eq!(bytes(SetColors(neither)), "");
}

#[test]
fn attributes_have_their_sgr_codes() {
    let attributes = [
        (Attribute::Reset, "0"),
        (Attribute::Bold, "1"),
        (Attribute::Dim, "2"),
        (Attribute::Italic, "3"),
        (Attribute::Underlined, "4"),
        (Attribute::DoubleUnderlined, "4:2"),
        (Attribute::Undercurled, "4:3"),
        (Attribute::Underdotted, "4:4"),
        (Attribute::Underdashed, "4:5"),
        (Attribute::SlowBlink, "5"),
        (Attribute::RapidBlink, "6"),
        (Attribute::Reverse, "7"),
        (Attribute::Hidden, "8"),
        (Attribute::CrossedOut, "9"),
        (Attribute::Fraktur, "20"),
        (Attribute::NoBold, "21"),
        (Attribute::NormalIntensity, "22"),
        (Attribute::NoItalic, "23"),
        (Attribute::NoUnderline, "24"),
        (Attribute::NoBlink, "25"),
        (Attribute::NoReverse, "27"),
        (Attribute::NoHidden, "28"),
        (Attribute::NotCrossedOut, "29"),
        (Attribute::Framed, "51"),
        (Attribute::Encircled, "52"),
        (Attribute::OverLined, "53"),
        (Attribute::NotFramedOrEncircled, "54"),
        (Attribute::NotOverLined, "55"),
    ];
    for (attribute, sgr) in attributes {
        assert_eq!(bytes(SetAttribute(attribute)), format!("\x1B[{sgr}m"));
    }
}

#[test]
fn print_writes_the_text() {
    assert_eq!(bytes(Print("héllo")), "héllo");
    assert_eq!(bytes(Print(42)), "42");
}
