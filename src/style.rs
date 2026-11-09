//! Colours and text attributes, and printing, as [`Command`]s with
//! crossterm's names and bytes.
//!
//! Unlike crossterm, `NO_COLOR` is not read here: whether to colour is the
//! program's choice, made before it writes a colour.

use std::fmt;

use crate::command::Command;

#[cfg(test)]
mod tests;

/// A colour, crossterm's: the sixteen named ones as palette entries 0 to 15,
/// any of the 256, or RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Color {
    /// The terminal's default.
    Reset,
    /// Palette 0.
    Black,
    /// Palette 8.
    DarkGrey,
    /// Palette 9.
    Red,
    /// Palette 1.
    DarkRed,
    /// Palette 10.
    Green,
    /// Palette 2.
    DarkGreen,
    /// Palette 11.
    Yellow,
    /// Palette 3.
    DarkYellow,
    /// Palette 12.
    Blue,
    /// Palette 4.
    DarkBlue,
    /// Palette 13.
    Magenta,
    /// Palette 5.
    DarkMagenta,
    /// Palette 14.
    Cyan,
    /// Palette 6.
    DarkCyan,
    /// Palette 15.
    White,
    /// Palette 7.
    Grey,
    /// RGB.
    Rgb {
        /// Red.
        r: u8,
        /// Green.
        g: u8,
        /// Blue.
        b: u8,
    },
    /// An entry of the 256-colour palette.
    AnsiValue(u8),
}

impl Color {
    /// Writes the colour's SGR parameters after `38;`, `48;` or `58;`;
    /// nothing for [`Color::Reset`], which has a code of its own.
    fn write_spec(self, f: &mut impl fmt::Write) -> fmt::Result {
        let index = match self {
            Self::Reset => return Ok(()),
            Self::Rgb { r, g, b } => return write!(f, "2;{r};{g};{b}"),
            Self::AnsiValue(n) => n,
            Self::Black => 0,
            Self::DarkRed => 1,
            Self::DarkGreen => 2,
            Self::DarkYellow => 3,
            Self::DarkBlue => 4,
            Self::DarkMagenta => 5,
            Self::DarkCyan => 6,
            Self::Grey => 7,
            Self::DarkGrey => 8,
            Self::Red => 9,
            Self::Green => 10,
            Self::Yellow => 11,
            Self::Blue => 12,
            Self::Magenta => 13,
            Self::Cyan => 14,
            Self::White => 15,
        };
        write!(f, "5;{index}")
    }

    /// The SGR parameters setting this colour on the layer whose code is
    /// `set` (38, 48 or 58), or resetting it with `set + 1`.
    fn write_sgr(self, set: u8, f: &mut impl fmt::Write) -> fmt::Result {
        if self == Self::Reset {
            return write!(f, "{}", set + 1);
        }
        write!(f, "{set};")?;
        self.write_spec(f)
    }
}

/// The SGR code setting the foreground; the background's is 48 and the
/// underline's 58, each reset by the code after it.
const FOREGROUND: u8 = 38;
const BACKGROUND: u8 = 48;
const UNDERLINE: u8 = 58;

/// Sets the foreground colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetForegroundColor(pub Color);

impl Command for SetForegroundColor {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1B[")?;
        self.0.write_sgr(FOREGROUND, f)?;
        f.write_str("m")
    }
}

/// Sets the background colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetBackgroundColor(pub Color);

impl Command for SetBackgroundColor {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1B[")?;
        self.0.write_sgr(BACKGROUND, f)?;
        f.write_str("m")
    }
}

/// Sets the underline's colour (SGR 58).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetUnderlineColor(pub Color);

impl Command for SetUnderlineColor {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1B[")?;
        self.0.write_sgr(UNDERLINE, f)?;
        f.write_str("m")
    }
}

/// A foreground and a background, either left as it is when `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Colors {
    /// The foreground.
    pub foreground: Option<Color>,
    /// The background.
    pub background: Option<Color>,
}

impl Colors {
    /// Both colours.
    pub const fn new(foreground: Color, background: Color) -> Self {
        Self {
            foreground: Some(foreground),
            background: Some(background),
        }
    }
}

/// Sets the foreground and background in one sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetColors(pub Colors);

impl Command for SetColors {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        let Colors {
            foreground,
            background,
        } = self.0;
        if foreground.is_none() && background.is_none() {
            return Ok(());
        }
        f.write_str("\x1B[")?;
        if let Some(color) = foreground {
            color.write_sgr(FOREGROUND, f)?;
        }
        if let Some(color) = background {
            if foreground.is_some() {
                f.write_str(";")?;
            }
            color.write_sgr(BACKGROUND, f)?;
        }
        f.write_str("m")
    }
}

/// Puts the foreground and background back to the terminal's, and every
/// attribute off (`CSI 0 m`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResetColor;

impl Command for ResetColor {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1B[0m")
    }
}

/// A text attribute, crossterm's, with its SGR code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Attribute {
    /// Every attribute and colour off (0).
    Reset,
    /// Bold (1).
    Bold,
    /// Dim (2).
    Dim,
    /// Italic (3).
    Italic,
    /// Underlined (4).
    Underlined,
    /// Double underlined (`4:2`).
    DoubleUnderlined,
    /// Curly underlined (`4:3`).
    Undercurled,
    /// Dotted underlined (`4:4`).
    Underdotted,
    /// Dashed underlined (`4:5`).
    Underdashed,
    /// Blinking, slowly (5).
    SlowBlink,
    /// Blinking, quickly (6).
    RapidBlink,
    /// Foreground and background swapped (7).
    Reverse,
    /// Hidden (8).
    Hidden,
    /// Crossed out (9).
    CrossedOut,
    /// Fraktur (20).
    Fraktur,
    /// Bold off, or double underlined on some terminals (21).
    NoBold,
    /// Neither bold nor dim (22).
    NormalIntensity,
    /// Italic off (23).
    NoItalic,
    /// Every underline off (24).
    NoUnderline,
    /// Blinking off (25).
    NoBlink,
    /// Reverse off (27).
    NoReverse,
    /// Hidden off (28).
    NoHidden,
    /// Crossed out off (29).
    NotCrossedOut,
    /// Framed (51).
    Framed,
    /// Encircled (52).
    Encircled,
    /// Overlined (53).
    OverLined,
    /// Framed and encircled off (54).
    NotFramedOrEncircled,
    /// Overlined off (55).
    NotOverLined,
}

impl Attribute {
    /// The attribute's SGR parameter.
    pub const fn sgr(self) -> &'static str {
        match self {
            Self::Reset => "0",
            Self::Bold => "1",
            Self::Dim => "2",
            Self::Italic => "3",
            Self::Underlined => "4",
            Self::DoubleUnderlined => "4:2",
            Self::Undercurled => "4:3",
            Self::Underdotted => "4:4",
            Self::Underdashed => "4:5",
            Self::SlowBlink => "5",
            Self::RapidBlink => "6",
            Self::Reverse => "7",
            Self::Hidden => "8",
            Self::CrossedOut => "9",
            Self::Fraktur => "20",
            Self::NoBold => "21",
            Self::NormalIntensity => "22",
            Self::NoItalic => "23",
            Self::NoUnderline => "24",
            Self::NoBlink => "25",
            Self::NoReverse => "27",
            Self::NoHidden => "28",
            Self::NotCrossedOut => "29",
            Self::Framed => "51",
            Self::Encircled => "52",
            Self::OverLined => "53",
            Self::NotFramedOrEncircled => "54",
            Self::NotOverLined => "55",
        }
    }
}

/// Sets an attribute (`CSI sgr m`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetAttribute(pub Attribute);

impl Command for SetAttribute {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(f, "\x1B[{}m", self.0.sgr())
    }
}

/// Prints text as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Print<T: fmt::Display>(pub T);

impl<T: fmt::Display> Command for Print<T> {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
