//! Size and raw mode on a pty.

use rustix::termios::{LocalModes, Termios, Winsize, tcgetattr, tcsetwinsize};

use super::WindowSize;
use super::{disable_raw_mode, enable_raw_mode_on, is_raw_mode_enabled, window_size_of};
use crate::test_pty::{pty, terminal_state as serial};

/// The attributes raw mode changes, comparable.
fn modes(termios: &Termios) -> impl PartialEq + std::fmt::Debug {
    (
        termios.input_modes,
        termios.output_modes,
        termios.control_modes,
        termios.local_modes,
    )
}

#[test]
fn window_size_reads_cells_and_pixels() {
    let (term, tty) = pty();
    let size = Winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 800,
        ws_ypixel: 480,
    };
    tcsetwinsize(&term, size).unwrap();
    let expected = WindowSize {
        columns: 80,
        rows: 24,
        width: 800,
        height: 480,
    };
    assert_eq!(window_size_of(&tty).unwrap(), expected);
}

#[test]
fn raw_mode_turns_off_line_editing_and_restores_exactly() {
    let _serial = serial();
    let (_term, tty) = pty();
    let before = tcgetattr(&tty).unwrap();
    assert!(
        before
            .local_modes
            .contains(LocalModes::ICANON | LocalModes::ECHO)
    );
    enable_raw_mode_on(|| tty.try_clone()).unwrap();
    assert!(is_raw_mode_enabled());
    let raw = tcgetattr(&tty).unwrap();
    assert!(
        !raw.local_modes
            .intersects(LocalModes::ICANON | LocalModes::ECHO | LocalModes::ISIG)
    );
    disable_raw_mode().unwrap();
    assert!(!is_raw_mode_enabled());
    assert_eq!(modes(&tcgetattr(&tty).unwrap()), modes(&before));
}

#[test]
fn raw_mode_twice_keeps_the_first_attributes() {
    let _serial = serial();
    let (_term, tty) = pty();
    let before = tcgetattr(&tty).unwrap();
    enable_raw_mode_on(|| tty.try_clone()).unwrap();
    // Raw already: the terminal is not opened again, nor raw taken as the
    // attributes to put back.
    enable_raw_mode_on(|| panic!("opened again")).unwrap();
    disable_raw_mode().unwrap();
    disable_raw_mode().unwrap();
    assert_eq!(modes(&tcgetattr(&tty).unwrap()), modes(&before));
}
