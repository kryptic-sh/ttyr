//! Size and raw mode on a pty.

use rustix::termios::{LocalModes, Termios, Winsize, tcgetattr, tcsetwinsize};

use super::WindowSize;
use super::{
    disable_raw_mode, enable_raw_mode_with, is_raw_mode_enabled, window_size_of, window_size_with,
};
use crate::test_pty::{local_modes, pty, terminal_state as serial};

/// The attributes raw mode changes, comparable.
fn modes(termios: &Termios) -> impl PartialEq + std::fmt::Debug {
    (
        termios.input_modes,
        termios.output_modes,
        termios.control_modes,
        local_modes(termios),
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
    enable_raw_mode_with(|| tty.try_clone()).unwrap();
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
    enable_raw_mode_with(|| tty.try_clone()).unwrap();
    // Raw already: the terminal is not opened again, nor raw taken as the
    // attributes to put back.
    enable_raw_mode_with(|| panic!("opened again")).unwrap();
    disable_raw_mode().unwrap();
    disable_raw_mode().unwrap();
    assert_eq!(modes(&tcgetattr(&tty).unwrap()), modes(&before));
}

/// tmxr's client under a launcher that leaves it no controlling terminal:
/// raw mode and the size on the terminal it was given.
#[test]
fn raw_mode_and_size_on_a_given_terminal() {
    let _serial = serial();
    let (term, tty) = pty();
    tcsetwinsize(
        &term,
        Winsize {
            ws_row: 30,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        },
    )
    .unwrap();
    assert_eq!(super::size_of(&tty).unwrap(), (100, 30));
    let before = tcgetattr(&tty).unwrap();
    super::enable_raw_mode_on(&tty).unwrap();
    let raw = tcgetattr(&tty).unwrap();
    assert!(!raw.local_modes.contains(LocalModes::ICANON));
    // The duplicate kept is what raw mode restores, after `tty` itself is
    // gone.
    let probe = tty.try_clone().unwrap();
    drop(tty);
    disable_raw_mode().unwrap();
    assert_eq!(modes(&tcgetattr(&probe).unwrap()), modes(&before));
}

/// A pty of `columns` by `rows`: the terminal's side, and the program's.
fn sized_pty(columns: u16, rows: u16) -> (std::fs::File, std::os::fd::OwnedFd) {
    let (term, tty) = pty();
    let size = Winsize {
        ws_row: rows,
        ws_col: columns,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    tcsetwinsize(&term, size).unwrap();
    (term, tty)
}

#[test]
fn the_size_is_the_controlling_terminals_where_it_opens() {
    let (_term, tty) = sized_pty(80, 24);
    let (_other, stdout) = sized_pty(132, 50);
    let size = window_size_with(|| Ok(tty), &stdout).unwrap();
    assert_eq!((size.columns, size.rows), (80, 24));
}

#[test]
fn with_no_controlling_terminal_the_size_is_stdouts() {
    let (_term, stdout) = sized_pty(132, 50);
    let no_tty = || {
        Err(std::io::Error::from_raw_os_error(
            rustix::io::Errno::NXIO.raw_os_error(),
        ))
    };
    let size = window_size_with(no_tty, &stdout).unwrap();
    assert_eq!((size.columns, size.rows), (132, 50));
}

#[test]
fn with_no_terminal_at_all_the_size_is_an_error() {
    let no_tty = || {
        Err(std::io::Error::from_raw_os_error(
            rustix::io::Errno::NXIO.raw_os_error(),
        ))
    };
    let (stdout, _write) = std::os::unix::net::UnixStream::pair().unwrap();
    assert!(window_size_with(no_tty, &stdout).is_err());
}
