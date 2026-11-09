//! The screen: clearing, scrolling, line wrap and the window's title, as [`Command`]s with
//! crossterm's names and bytes.

use std::fmt;

use crate::command::{Command, fixed_commands};

#[cfg(test)]
mod tests;

/// What [`Clear`] clears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClearType {
    /// The whole screen (`CSI 2 J`).
    All,
    /// The scrollback (`CSI 3 J`).
    Purge,
    /// From the cursor to the screen's end (`CSI J`).
    FromCursorDown,
    /// From the screen's start to the cursor (`CSI 1 J`).
    FromCursorUp,
    /// The cursor's line (`CSI 2 K`).
    CurrentLine,
    /// From the cursor to the line's end (`CSI K`).
    UntilNewLine,
    /// From the line's start to the cursor (`CSI 1 K`); crossterm has no
    /// such clear.
    FromLineStart,
}

/// Clears part of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Clear(pub ClearType);

impl Command for Clear {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str(match self.0 {
            ClearType::All => "\x1B[2J",
            ClearType::Purge => "\x1B[3J",
            ClearType::FromCursorDown => "\x1B[J",
            ClearType::FromCursorUp => "\x1B[1J",
            ClearType::CurrentLine => "\x1B[2K",
            ClearType::UntilNewLine => "\x1B[K",
            ClearType::FromLineStart => "\x1B[1K",
        })
    }
}

/// Scrolls the screen (or the scroll region) up a number of lines
/// (`CSI n S`); zero writes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScrollUp(pub u16);

impl Command for ScrollUp {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        if self.0 == 0 {
            return Ok(());
        }
        write!(f, "\x1B[{}S", self.0)
    }
}

/// Scrolls the screen (or the scroll region) down a number of lines
/// (`CSI n T`); zero writes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScrollDown(pub u16);

impl Command for ScrollDown {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        if self.0 == 0 {
            return Ok(());
        }
        write!(f, "\x1B[{}T", self.0)
    }
}

/// Limits scrolling to the rows `top` to `bottom`, both included and
/// zero-based (DECSTBM, `CSI top ; bottom r`). It also moves the cursor to
/// the screen's start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetScrollRegion {
    /// The region's first row.
    pub top: u16,
    /// The region's last row.
    pub bottom: u16,
}

impl Command for SetScrollRegion {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(
            f,
            "\x1B[{};{}r",
            u32::from(self.top) + 1,
            u32::from(self.bottom) + 1
        )
    }
}

/// Sets the window's title (`OSC 0 ; title BEL`, as crossterm writes it).
/// Control characters in the title end the sequence early, so leave them
/// out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetTitle<T: fmt::Display>(pub T);

impl<T: fmt::Display> Command for SetTitle<T> {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(f, "\x1B]0;{}\x07", self.0)
    }
}

fixed_commands! {
    /// Lets the whole screen scroll again (`CSI r`).
    ResetScrollRegion => "\x1B[r";
    /// Stops text wrapping at the line's end (DECAWM off, `CSI ? 7 l`).
    DisableLineWrap => "\x1B[?7l";
    /// Wraps text at the line's end again (DECAWM on, `CSI ? 7 h`).
    EnableLineWrap => "\x1B[?7h";
}
