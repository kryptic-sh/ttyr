//! Queries to the terminal, each a [`Command`] that only writes the
//! question: the answer arrives later as an `Event::Report` (02's
//! [`Report`](crate::event::Report)), read like any other event, so nothing
//! blocks on a terminal that never answers.
//!
//! Every terminal answers [`RequestPrimaryDeviceAttributes`], and answers in
//! order, so writing it last marks the end of the answers: one that has not
//! come by its answer is not coming.

use std::fmt;

use crate::command::Command;
use crate::event::ColorSlot;

#[cfg(test)]
mod tests;

/// Asks for the kitty keyboard flags in force (`CSI ? u`); the answer is
/// `Report::KeyboardEnhancementFlags`, and only a terminal that speaks the
/// kitty protocol sends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestKeyboardEnhancementFlags;

impl Command for RequestKeyboardEnhancementFlags {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1B[?u")
    }
}

/// Asks for the primary device attributes (DA1, `CSI c`); the answer is
/// `Report::PrimaryDeviceAttributes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestPrimaryDeviceAttributes;

impl Command for RequestPrimaryDeviceAttributes {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1B[c")
    }
}

/// Asks for the secondary device attributes (DA2, `CSI > c`); the answer is
/// `Report::SecondaryDeviceAttributes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestSecondaryDeviceAttributes;

impl Command for RequestSecondaryDeviceAttributes {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1B[>c")
    }
}

/// Asks for the terminal's name and version (XTVERSION, `CSI > q`); the
/// answer is `Report::TerminalVersion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestTerminalVersion;

impl Command for RequestTerminalVersion {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1B[>q")
    }
}

/// Asks for a mode's setting (DECRQM: `CSI ? mode $ p` for a DEC private
/// mode, `CSI mode $ p` for an ANSI one); the answer is `Report::Mode`.
/// Asking for 2026 says whether the terminal has synchronized output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestMode {
    /// The mode's number.
    pub mode: u16,
    /// Whether it is a DEC private mode.
    pub private: bool,
}

impl Command for RequestMode {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        let private = if self.private { "?" } else { "" };
        write!(f, "\x1B[{private}{}$p", self.mode)
    }
}

/// Asks for one of the terminal's colours (`OSC 10 ; ?`, `OSC 11 ; ?`,
/// `OSC 12 ; ?` or `OSC 4 ; n ; ?`, ended with ST); the answer is
/// `Report::TerminalColor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestTerminalColor(pub ColorSlot);

impl Command for RequestTerminalColor {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        match self.0 {
            ColorSlot::Foreground => f.write_str("\x1B]10;?\x1B\\"),
            ColorSlot::Background => f.write_str("\x1B]11;?\x1B\\"),
            ColorSlot::Cursor => f.write_str("\x1B]12;?\x1B\\"),
            ColorSlot::Palette(n) => write!(f, "\x1B]4;{n};?\x1B\\"),
        }
    }
}

/// Asks for the text area's size in pixels (`CSI 14 t`); the answer is
/// `Report::TextAreaPixels`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestTextAreaPixels;

impl Command for RequestTextAreaPixels {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1B[14t")
    }
}
