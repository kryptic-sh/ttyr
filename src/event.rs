//! Events a terminal sends, and the [`Decoder`] that reads them from bytes.

pub(crate) mod console;
mod decoder;
mod key;
mod mouse;
mod report;

pub use decoder::Decoder;
pub use key::{
    KeyChord, KeyCode, KeyEncoding, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
    MediaKeyCode, ModifierKeyCode,
};
pub use mouse::{MouseButton, MouseEvent, MouseEventKind};
pub use report::{
    ColorScheme, ColorSlot, KeyboardEnhancementFlags, ModeSetting, Report, TerminalColor,
};

/// An event from the terminal.
///
/// With the `serde` feature, every event type is `Serialize` and
/// `Deserialize` by plain derive: a non-self-describing format such as
/// postcard encodes variants by index and fields by order, so a change to
/// these types changes that encoding, and both ends must be built against the
/// same ttyr.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
#[non_exhaustive]
pub enum Event {
    /// The terminal gained focus (mode 1004).
    FocusGained,
    /// The terminal lost focus (mode 1004).
    FocusLost,
    /// A key.
    Key(KeyEvent),
    /// The mouse.
    Mouse(MouseEvent),
    /// Pasted text (bracketed paste, mode 2004). Bytes that are not UTF-8 are
    /// replaced with U+FFFD.
    Paste(String),
    /// The terminal's new size, columns then rows. The reader sends it on
    /// `SIGWINCH` on Unix, and on Windows for the console's resize records,
    /// which come once raw mode has turned `ENABLE_WINDOW_INPUT` on; the
    /// decoder never does.
    Resize(u16, u16),
    /// The reader's `Waker` woke it (`ttyr::reader`); the decoder never
    /// sends it.
    Wake,
    /// A report: an answer to a query, or a change the terminal announces.
    Report(Report),
    /// Bytes that are not an event ttyr knows: a well-formed sequence it does
    /// not decode, or bytes that are not well formed, passed on whole so that
    /// nothing after them is lost.
    Unknown(Vec<u8>),
}
