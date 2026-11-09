//! The modes a program turns on and off, each a [`Command`] with its
//! sequence, under crossterm's names where crossterm has the mode.

use std::fmt;

use crate::command::{Command, fixed_commands};
use crate::event::KeyboardEnhancementFlags;

#[cfg(test)]
mod tests;

fixed_commands! {
    /// Switches to the alternate screen (mode 1049), saving the cursor.
    EnterAlternateScreen => "\x1B[?1049h";
    /// Switches back to the main screen, restoring the cursor.
    LeaveAlternateScreen => "\x1B[?1049l";
    /// Reports every mouse button, drag and motion (modes 1000, 1002 and
    /// 1003), in SGR's form (1006), as crossterm's `EnableMouseCapture`.
    /// [`SetMouseTracking`] picks one level.
    EnableMouseCapture => "\x1B[?1000h\x1B[?1002h\x1B[?1003h\x1B[?1006h";
    /// Stops every mouse report [`EnableMouseCapture`] or
    /// [`SetMouseTracking`] turns on.
    DisableMouseCapture => "\x1B[?1006l\x1B[?1003l\x1B[?1002l\x1B[?1000l";
    /// Wraps pasted text in `CSI 200 ~` … `CSI 201 ~` (mode 2004), read as
    /// `Event::Paste`.
    EnableBracketedPaste => "\x1B[?2004h";
    /// Stops wrapping pasted text.
    DisableBracketedPaste => "\x1B[?2004l";
    /// Reports focus gained and lost (mode 1004).
    EnableFocusChange => "\x1B[?1004h";
    /// Stops reporting focus.
    DisableFocusChange => "\x1B[?1004l";
    /// Pops the kitty keyboard flags [`PushKeyboardEnhancementFlags`] pushed
    /// (`CSI < 1 u`).
    PopKeyboardEnhancementFlags => "\x1B[<1u";
    /// xterm's modifyOtherKeys level 2 (`CSI > 4 ; 2 m`): keys that would be
    /// ambiguous arrive as `CSI 27 ; mods ; key ~`.
    EnableModifyOtherKeys => "\x1B[>4;2m";
    /// modifyOtherKeys back to the terminal's default (`CSI > 4 m`).
    DisableModifyOtherKeys => "\x1B[>4m";
    /// Reports the colour scheme each time it changes (mode 2031), as
    /// `Report::ColorScheme`. It does not report the scheme now; ask with
    /// [`RequestColorScheme`].
    EnableColorSchemeReports => "\x1B[?2031h";
    /// Stops the colour scheme reports.
    DisableColorSchemeReports => "\x1B[?2031l";
    /// Asks for the colour scheme now (`CSI ? 996 n`); the answer arrives as
    /// `Report::ColorScheme`.
    RequestColorScheme => "\x1B[?996n";
    /// Windows Terminal's win32-input-mode (`CSI ? 9001 h`): every key as a
    /// console key record, `CSI Vk ; Sc ; Uc ; Kd ; Cs ; Rc _`, read as
    /// `KeyEncoding::Windows`. Other terminals ignore it.
    EnableWin32InputMode => "\x1B[?9001h";
    /// Ends win32-input-mode.
    DisableWin32InputMode => "\x1B[?9001l";
    /// Holds the screen until [`EndSynchronizedUpdate`] (mode 2026), so a
    /// frame is never shown half drawn.
    BeginSynchronizedUpdate => "\x1B[?2026h";
    /// Shows what was drawn since [`BeginSynchronizedUpdate`].
    EndSynchronizedUpdate => "\x1B[?2026l";
}

/// Pushes kitty keyboard flags onto the terminal's stack (`CSI > flags u`);
/// [`PopKeyboardEnhancementFlags`] puts back the flags from before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PushKeyboardEnhancementFlags(pub KeyboardEnhancementFlags);

impl Command for PushKeyboardEnhancementFlags {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(f, "\x1B[>{}u", self.0.bits())
    }
}

/// Replaces the kitty keyboard flags on the top of the terminal's stack
/// (`CSI = flags ; 1 u`), pushing nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetKeyboardEnhancementFlags(pub KeyboardEnhancementFlags);

impl Command for SetKeyboardEnhancementFlags {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(f, "\x1B[={};1u", self.0.bits())
    }
}

/// Which mouse events the terminal reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseTracking {
    /// Presses, releases and the wheel (mode 1000).
    Normal,
    /// Those and motion while a button is held (mode 1002).
    Button,
    /// Those and all motion (mode 1003).
    Any,
}

impl MouseTracking {
    /// The mode's number.
    const fn mode(self) -> u16 {
        match self {
            Self::Normal => 1000,
            Self::Button => 1002,
            Self::Any => 1003,
        }
    }
}

/// Turns on one level of mouse reports, and SGR's form for them (1006) or
/// not. It turns nothing else off: to change levels, write
/// [`DisableMouseCapture`] first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetMouseTracking {
    /// Which events.
    pub tracking: MouseTracking,
    /// Whether in SGR's form, which has no limit on the column and row and
    /// says which button was released.
    pub sgr: bool,
}

impl Command for SetMouseTracking {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(f, "\x1B[?{}h", self.tracking.mode())?;
        if self.sgr {
            f.write_str("\x1B[?1006h")?;
        }
        Ok(())
    }
}
