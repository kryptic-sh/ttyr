//! The console's mouse records as mouse events.

use super::modifiers;
use crate::event::{MouseButton, MouseEvent, MouseEventKind};

#[cfg(test)]
mod tests;

/// A console mouse record's fields, as `MOUSE_EVENT_RECORD` has them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseRecord {
    /// The column, in the screen buffer (`dwMousePosition.X`).
    pub x: i16,
    /// The row, in the screen buffer (`dwMousePosition.Y`).
    pub y: i16,
    /// The buttons down, and with a wheel the distance in the high word
    /// (`dwButtonState`).
    pub buttons: u32,
    /// The modifier and lock state (`dwControlKeyState`).
    pub state: u32,
    /// What happened (`dwEventFlags`).
    pub flags: u32,
}

// `dwEventFlags` values, fixed by the Windows API (wincon.h).
const MOUSE_MOVED: u32 = 0x0001;
const MOUSE_WHEELED: u32 = 0x0004;
const MOUSE_HWHEELED: u32 = 0x0008;

/// The buttons in `dwButtonState`'s low bits, in the order they are
/// reported (wincon.h: `FROM_LEFT_1ST_BUTTON_PRESSED` is 1,
/// `RIGHTMOST_BUTTON_PRESSED` 2, then the 2nd, 3rd and 4th from the left).
const BUTTONS: [(u32, MouseButton); 5] = [
    (0x0001, MouseButton::Left),
    (0x0002, MouseButton::Right),
    (0x0004, MouseButton::Middle),
    (0x0008, MouseButton::Back),
    (0x0010, MouseButton::Forward),
];

/// Turns console mouse records into mouse events, keeping the buttons held,
/// as a record says which are down, not which changed.
#[derive(Debug, Clone, Default)]
pub struct ConsoleMouse {
    held: u32,
}

impl ConsoleMouse {
    /// The events `record` is: one per button that went down or up, else a
    /// drag, a move or a wheel's turn. `top` is the screen buffer's row at
    /// the window's top, as rows count from the window.
    pub fn events(&mut self, record: &MouseRecord, top: i16) -> Vec<MouseEvent> {
        let event = |kind| MouseEvent {
            kind,
            column: u16::try_from(record.x).unwrap_or(0),
            row: u16::try_from(i32::from(record.y) - i32::from(top)).unwrap_or(0),
            modifiers: modifiers(record.state),
        };
        // The wheel's distance is the high word, signed: away from the user
        // (up, or right) is positive.
        let away = record.buttons.to_be_bytes()[0] & 0x80 == 0;
        if record.flags & MOUSE_WHEELED != 0 {
            let kind = if away {
                MouseEventKind::ScrollUp
            } else {
                MouseEventKind::ScrollDown
            };
            return vec![event(kind)];
        }
        if record.flags & MOUSE_HWHEELED != 0 {
            let kind = if away {
                MouseEventKind::ScrollRight
            } else {
                MouseEventKind::ScrollLeft
            };
            return vec![event(kind)];
        }
        let down = record.buttons & BUTTONS.iter().fold(0, |all, (bit, _)| all | bit);
        let changed = down ^ self.held;
        self.held = down;
        let mut events: Vec<_> = BUTTONS
            .iter()
            .filter(|(bit, _)| changed & bit != 0)
            .map(|&(bit, button)| {
                event(if down & bit != 0 {
                    MouseEventKind::Down(button)
                } else {
                    MouseEventKind::Up(button)
                })
            })
            .collect();
        if events.is_empty() && record.flags & MOUSE_MOVED != 0 {
            let held = BUTTONS.iter().find(|(bit, _)| down & bit != 0);
            events.push(event(held.map_or(MouseEventKind::Moved, |&(_, button)| {
                MouseEventKind::Drag(button)
            })));
        }
        events
    }
}
