//! Size and raw mode on a pty.

use rustix::termios::{LocalModes, Termios, Winsize, tcgetattr, tcsetwinsize};

use super::{
    disable_raw_mode, enable_raw_mode_with, is_raw_mode_enabled, window_size_of, window_size_with,
};
use crate::terminal::WindowSize;
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

/// Set in the child [`the_controlling_terminal_entry_points`] runs itself
/// as, to the path of the terminal to make its controlling one.
const CHILD_TTY: &str = "TTYR_TEST_CONTROLLING_TTY";

/// What the child says once it reads keys, so the test types one.
const READY: &str = "ttyr-test-ready";

/// The entry points that open `/dev/tty` (`size`, `window_size`,
/// `enable_raw_mode`, `Reader::open`, the panic hook's fallback output and
/// the backend's size), which need a controlling terminal: the test binary
/// runs this test again as a child in a session of its own on a pty.
#[test]
fn the_controlling_terminal_entry_points() {
    match std::env::var(CHILD_TTY) {
        Ok(path) => controlling_terminal_child(&path),
        Err(_) => controlling_terminal_parent(),
    }
}

fn controlling_terminal_parent() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    // `_tty`, the program's side, is held to the end, so reading the
    // terminal's side waits for the child rather than failing before it
    // opens its own.
    let (mut term, _tty) = pty();
    let size = Winsize {
        ws_row: 33,
        ws_col: 111,
        ws_xpixel: 999,
        ws_ypixel: 666,
    };
    tcsetwinsize(&term, size).unwrap();
    let path = rustix::pty::ptsname(&term, Vec::new()).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "terminal::unix::tests::the_controlling_terminal_entry_points",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_TTY, path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (lines, lines_rx) = mpsc::channel();
    let stdout = child.stdout.take().unwrap();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let _ = lines.send(line.unwrap_or_default());
        }
    });
    let mut stderr = child.stderr.take().unwrap();
    let errors = thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let mut output = term.try_clone().unwrap();
    let (chunks, chunks_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = [0; 256];
        while let Ok(n @ 1..) = output.read(&mut buf) {
            let _ = chunks.send(buf[..n].to_vec());
        }
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut said = Vec::new();
    let ready = loop {
        match lines_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            // After libtest's "test … " on the same line.
            Ok(line) if line.ends_with(READY) => break true,
            Ok(line) => said.push(line),
            Err(_) => break false,
        }
    };
    // The panic hook's off sequences, which it writes through `/dev/tty`
    // once raw mode is off; the child waits for a line after them.
    let off = b"\x1B[?2004l\x1B[?25h";
    let mut written = Vec::new();
    if ready {
        term.write_all(b"x").unwrap();
        while !written.ends_with(off) {
            let left = deadline.saturating_duration_since(Instant::now());
            match chunks_rx.recv_timeout(left) {
                Ok(chunk) => written.extend(chunk),
                Err(_) => break,
            }
        }
        term.write_all(b"\n").unwrap();
    }
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let errors = errors.join().unwrap();
    assert!(ready, "the child never read keys:\n{said:?}\n{errors}");
    assert!(status.is_some_and(|s| s.success()), "{status:?}:\n{errors}");
    assert!(
        written.ends_with(off),
        "{:?}",
        String::from_utf8_lossy(&written)
    );
}

fn controlling_terminal_child(path: &str) {
    use std::io::Write;

    use rustix::fs::{Mode, OFlags};

    use crate::event::{Event, KeyCode, KeyEncoding, KeyEvent, KeyModifiers};
    use crate::reader::Reader;
    use crate::terminal::{self, enable_raw_mode, restore_on_panic, size, window_size};

    rustix::process::setsid().unwrap();
    let tty = rustix::fs::open(path, OFlags::RDWR | OFlags::NOCTTY, Mode::empty()).unwrap();
    rustix::process::ioctl_tiocsctty(&tty).unwrap();

    assert_eq!(size().unwrap(), (111, 33));
    let expected = WindowSize {
        columns: 111,
        rows: 33,
        width: 999,
        height: 666,
    };
    assert_eq!(window_size().unwrap(), expected);
    #[cfg(feature = "ratatui")]
    {
        use ratatui_core::backend::Backend;
        let mut backend = crate::backend::TtyrBackend::new(Vec::new());
        let cells = ratatui_core::layout::Size {
            width: 111,
            height: 33,
        };
        assert_eq!(backend.size().unwrap(), cells);
        let window = backend.window_size().unwrap();
        assert_eq!(window.columns_rows, cells);
        assert_eq!((window.pixels.width, window.pixels.height), (999, 666));
    }

    enable_raw_mode().unwrap();
    assert!(
        !tcgetattr(&tty)
            .unwrap()
            .local_modes
            .contains(LocalModes::ICANON)
    );
    let mut reader = Reader::open().unwrap();
    println!("{READY}");
    std::io::stdout().flush().unwrap();
    let x =
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE).with_encoding(KeyEncoding::Legacy);
    assert_eq!(reader.read().unwrap(), Event::Key(x));
    drop(reader);
    disable_raw_mode().unwrap();
    assert!(
        tcgetattr(&tty)
            .unwrap()
            .local_modes
            .contains(LocalModes::ICANON)
    );

    // With raw mode off, the panic hook writes to `/dev/tty`.
    restore_on_panic();
    terminal::enable(&mut Vec::new(), terminal::Mode::BracketedPaste).unwrap();
    let panicked = std::panic::catch_unwind(|| panic!("on purpose"));
    assert!(panicked.is_err());
    // Until the test has read them.
    let mut line = [0; 1];
    rustix::io::read(&tty, &mut line).unwrap();
}
