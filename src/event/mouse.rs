//! Mouse events.

use super::KeyModifiers;

/// A mouse event.
///
/// `column` and `row` are zero-based. With SGR-pixels mouse reporting (mode
/// 1016) on, they are pixels instead: the decoder cannot tell, the caller
/// who turned the mode on can.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub struct MouseEvent {
    /// What happened.
    pub kind: MouseEventKind,
    /// The column.
    pub column: u16,
    /// The row.
    pub row: u16,
    /// Shift, Alt and Ctrl, as the terminal reports them.
    pub modifiers: KeyModifiers,
}

/// What a mouse event is.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum MouseEventKind {
    /// A button went down.
    Down(MouseButton),
    /// A button came up. The legacy encodings do not say which, and report
    /// [`MouseButton::Left`].
    Up(MouseButton),
    /// The mouse moved with a button down.
    Drag(MouseButton),
    /// The mouse moved with no button down (mode 1003).
    Moved,
    /// The wheel scrolled down.
    ScrollDown,
    /// The wheel scrolled up.
    ScrollUp,
    /// The wheel scrolled left.
    ScrollLeft,
    /// The wheel scrolled right.
    ScrollRight,
}

/// A mouse button.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
#[non_exhaustive]
pub enum MouseButton {
    /// The left button.
    Left,
    /// The right button.
    Right,
    /// The middle button.
    Middle,
    /// The back button (xterm's button 8).
    Back,
    /// The forward button (xterm's button 9).
    Forward,
}
