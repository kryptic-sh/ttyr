//! The cursor: moving it, showing and hiding it, its shape and colour, and
//! asking where it is, as [`Command`]s with crossterm's names and bytes.
//! Columns and rows are zero-based, as crossterm counts them.

use std::fmt;

use crate::command::{Command, fixed_commands};

#[cfg(test)]
mod tests;

/// Moves the cursor to a column and row (`CSI row ; column H`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoveTo(pub u16, pub u16);

impl Command for MoveTo {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(
            f,
            "\x1B[{};{}H",
            u32::from(self.1) + 1,
            u32::from(self.0) + 1
        )
    }
}

/// Moves the cursor to a column on its row (`CSI column G`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoveToColumn(pub u16);

impl Command for MoveToColumn {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(f, "\x1B[{}G", u32::from(self.0) + 1)
    }
}

/// Moves the cursor to a row in its column (`CSI row d`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoveToRow(pub u16);

impl Command for MoveToRow {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(f, "\x1B[{}d", u32::from(self.0) + 1)
    }
}

/// Cursor commands that move it a number of cells or lines with one final
/// byte.
macro_rules! relative_moves {
    ($($(#[$doc:meta])* $name:ident => $final:literal;)*) => {$(
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(pub u16);

        impl Command for $name {
            fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
                write!(f, concat!("\x1B[{}", $final), self.0)
            }
        }
    )*};
}

relative_moves! {
    /// Moves the cursor up (`CSI n A`).
    MoveUp => "A";
    /// Moves the cursor down (`CSI n B`).
    MoveDown => "B";
    /// Moves the cursor right (`CSI n C`).
    MoveRight => "C";
    /// Moves the cursor left (`CSI n D`).
    MoveLeft => "D";
    /// Moves the cursor down a number of lines, to the first column
    /// (`CSI n E`).
    MoveToNextLine => "E";
    /// Moves the cursor up a number of lines, to the first column (`CSI n F`).
    MoveToPreviousLine => "F";
}

fixed_commands! {
    /// Saves the cursor's position (DECSC, `ESC 7`).
    SavePosition => "\x1B7";
    /// Moves the cursor back to the position saved (DECRC, `ESC 8`).
    RestorePosition => "\x1B8";
    /// Hides the cursor (`CSI ? 25 l`).
    Hide => "\x1B[?25l";
    /// Shows the cursor (`CSI ? 25 h`).
    Show => "\x1B[?25h";
    /// Makes the cursor blink (`CSI ? 12 h`).
    EnableBlinking => "\x1B[?12h";
    /// Stops the cursor blinking (`CSI ? 12 l`).
    DisableBlinking => "\x1B[?12l";
    /// Asks where the cursor is (`CSI 6 n`); the answer is
    /// `Report::CursorPosition`, read like any other event.
    RequestCursorPosition => "\x1B[6n";
    /// Puts the cursor's colour back to the terminal's own (`OSC 112`).
    ResetCursorColor => "\x1B]112\x1B\\";
}

/// The cursor's shape (DECSCUSR, `CSI n SP q`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SetCursorStyle {
    /// The shape the user configured.
    DefaultUserShape,
    /// A blinking block.
    BlinkingBlock,
    /// A steady block.
    SteadyBlock,
    /// A blinking underscore.
    BlinkingUnderScore,
    /// A steady underscore.
    SteadyUnderScore,
    /// A blinking bar.
    BlinkingBar,
    /// A steady bar.
    SteadyBar,
}

impl Command for SetCursorStyle {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        let n = match self {
            Self::DefaultUserShape => 0,
            Self::BlinkingBlock => 1,
            Self::SteadyBlock => 2,
            Self::BlinkingUnderScore => 3,
            Self::SteadyUnderScore => 4,
            Self::BlinkingBar => 5,
            Self::SteadyBar => 6,
        };
        write!(f, "\x1B[{n} q")
    }
}

/// Sets the cursor's colour (`OSC 12 ; rgb:rr/gg/bb`); [`ResetCursorColor`]
/// puts the terminal's own back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetCursorColor {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Command for SetCursorColor {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(
            f,
            "\x1B]12;rgb:{:02x}/{:02x}/{:02x}\x1B\\",
            self.r, self.g, self.b
        )
    }
}
