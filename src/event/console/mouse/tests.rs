//! Console mouse records as mouse events.

use super::*;
use crate::event::KeyModifiers;

fn mouse(x: i16, y: i16, buttons: u32, flags: u32) -> MouseRecord {
    MouseRecord {
        x,
        y,
        buttons,
        state: 0,
        flags,
    }
}

fn kinds(mouse: &mut ConsoleMouse, record: MouseRecord) -> Vec<MouseEventKind> {
    mouse.events(&record, 0).iter().map(|e| e.kind).collect()
}

#[test]
fn mouse_buttons_go_down_drag_and_come_up() {
    use MouseEventKind::{Down, Drag, Moved, Up};
    let mut m = ConsoleMouse::default();
    assert_eq!(
        kinds(&mut m, mouse(1, 1, 0x1, 0)),
        [Down(MouseButton::Left)]
    );
    assert_eq!(
        kinds(&mut m, mouse(2, 1, 0x1, MOUSE_MOVED)),
        [Drag(MouseButton::Left)]
    );
    // The right button joins, then both come up in one record.
    assert_eq!(
        kinds(&mut m, mouse(2, 1, 0x3, 0)),
        [Down(MouseButton::Right)]
    );
    assert_eq!(
        kinds(&mut m, mouse(2, 1, 0x0, 0)),
        [Up(MouseButton::Left), Up(MouseButton::Right)]
    );
    assert_eq!(kinds(&mut m, mouse(3, 1, 0x0, MOUSE_MOVED)), [Moved]);
    // Where crossterm's console reader dropped them.
    assert_eq!(
        kinds(&mut m, mouse(3, 1, 0x8, 0)),
        [Down(MouseButton::Back)]
    );
    assert_eq!(
        kinds(&mut m, mouse(3, 1, 0x10, 0)),
        [Up(MouseButton::Back), Down(MouseButton::Forward)]
    );
}

#[test]
fn mouse_wheels_and_position() {
    use MouseEventKind::{ScrollDown, ScrollLeft, ScrollRight, ScrollUp};
    let mut m = ConsoleMouse::default();
    // 120 away from the user, then 120 towards (the high word, signed).
    let away = 120 << 16;
    let towards = u32::from((-120i16).cast_unsigned()) << 16;
    assert_eq!(kinds(&mut m, mouse(0, 0, away, MOUSE_WHEELED)), [ScrollUp]);
    assert_eq!(
        kinds(&mut m, mouse(0, 0, towards, MOUSE_WHEELED)),
        [ScrollDown]
    );
    assert_eq!(
        kinds(&mut m, mouse(0, 0, away, MOUSE_HWHEELED)),
        [ScrollRight]
    );
    assert_eq!(
        kinds(&mut m, mouse(0, 0, towards, MOUSE_HWHEELED)),
        [ScrollLeft]
    );
    // Rows count from the window's top, not the buffer's; left Ctrl is
    // wincon.h's LEFT_CTRL_PRESSED.
    let record = MouseRecord {
        state: 0x0008,
        ..mouse(7, 105, 0x1, 0)
    };
    let events = m.events(&record, 100);
    assert_eq!(
        events,
        [MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 7,
            row: 5,
            modifiers: KeyModifiers::CONTROL,
        }]
    );
}
