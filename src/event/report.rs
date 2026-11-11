//! The terminal's answers to queries, and the reports it sends unasked.

use bitflags::bitflags;

/// A report from the terminal.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
#[non_exhaustive]
pub enum Report {
    /// The kitty keyboard flags in force, `CSI ? flags u`, in answer to
    /// `CSI ? u`. A terminal that sends it speaks the kitty protocol.
    KeyboardEnhancementFlags(KeyboardEnhancementFlags),
    /// Primary device attributes (DA1), `CSI ? Ps ; … c`, in answer to
    /// `CSI c`. Every terminal answers it, so it marks the end of the answers
    /// to queries sent before it.
    PrimaryDeviceAttributes(Vec<u16>),
    /// Secondary device attributes (DA2), `CSI > Ps ; … c`, in answer to
    /// `CSI > c`: the terminal's type, version and options.
    SecondaryDeviceAttributes(Vec<u16>),
    /// The terminal's name and version (XTVERSION), `DCS > | text ST`, in
    /// answer to `CSI > q`.
    TerminalVersion(String),
    /// A mode's setting (DECRPM), `CSI ? Ps ; Pm $ y` for a DEC private mode
    /// or `CSI Ps ; Pm $ y` for an ANSI one, in answer to DECRQM (`CSI ? Ps $
    /// p`, `CSI Ps $ p`).
    Mode {
        /// The mode's number: 2026 is synchronized output.
        mode: u16,
        /// Whether it is a DEC private mode (`?`).
        private: bool,
        /// Its setting.
        setting: ModeSetting,
    },
    /// The terminal's colour scheme, `CSI ? 997 ; 1 n` dark or `CSI ? 997 ;
    /// 2 n` light: in answer to `CSI ? 996 n`, and on each change while mode
    /// 2031 is set.
    ColorScheme(ColorScheme),
    /// A colour the terminal reports in answer to a query: `OSC 10 ; ?` (its
    /// foreground), `OSC 11 ; ?` (background), `OSC 12 ; ?` (cursor) or
    /// `OSC 4 ; n ; ?` (palette entry `n`), answered in `rgb:` form.
    TerminalColor(TerminalColor),
    /// The cursor's position (CPR), `CSI row ; column R`, in answer to
    /// `CSI 6 n`; zero-based. xterm's Ctrl+F3 and the like (`CSI 1 ; mods R`)
    /// have the same form and are read as this.
    CursorPosition {
        /// The column.
        column: u16,
        /// The row.
        row: u16,
    },
    /// The text area's size in pixels, `CSI 4 ; height ; width t`, in answer
    /// to `CSI 14 t`.
    TextAreaPixels {
        /// Its width.
        width: u16,
        /// Its height.
        height: u16,
    },
}

bitflags! {
    /// The kitty keyboard protocol's progressive enhancement flags.
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
    pub struct KeyboardEnhancementFlags: u8 {
        /// 1: keys that would be ambiguous arrive as `CSI … u`.
        const DISAMBIGUATE_ESCAPE_CODES = 0b0000_0001;
        /// 2: repeat and release events.
        const REPORT_EVENT_TYPES = 0b0000_0010;
        /// 4: the shifted and base-layout keys.
        const REPORT_ALTERNATE_KEYS = 0b0000_0100;
        /// 8: every key as an escape code, text keys and modifiers included.
        const REPORT_ALL_KEYS_AS_ESCAPE_CODES = 0b0000_1000;
        /// 16: the text a key produced.
        const REPORT_ASSOCIATED_TEXT = 0b0001_0000;
        // Bits a later version of the protocol adds are kept.
        const _ = !0;
    }
}

/// A mode's setting, as DECRPM reports it.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
#[non_exhaustive]
pub enum ModeSetting {
    /// 0: the terminal does not know the mode.
    NotRecognized,
    /// 1: set.
    Set,
    /// 2: reset.
    Reset,
    /// 3: permanently set.
    PermanentlySet,
    /// 4: permanently reset.
    PermanentlyReset,
}

/// A terminal's colour scheme (see [`Report::ColorScheme`]).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
#[non_exhaustive]
pub enum ColorScheme {
    /// Dark.
    Dark,
    /// Light.
    Light,
}

/// A colour the terminal reported (see [`Report::TerminalColor`]).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub struct TerminalColor {
    /// Which colour it is.
    pub slot: ColorSlot,
    /// Its red, green and blue, each scaled to 16 bits as X11's `rgb:` form
    /// scales one to four hex digits.
    pub rgb: (u16, u16, u16),
}

/// Which of the terminal's colours a [`TerminalColor`] is.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
#[non_exhaustive]
pub enum ColorSlot {
    /// The default foreground (OSC 10).
    Foreground,
    /// The default background (OSC 11).
    Background,
    /// The cursor's (OSC 12).
    Cursor,
    /// An entry of the 256-colour palette (OSC 4).
    Palette(u8),
}
