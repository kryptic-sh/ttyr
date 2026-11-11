//! Ptys for the tests: the test writes to the terminal's side (the master),
//! as a terminal emulator would, and the code under test uses the program's.

use std::fs::File;
use std::os::fd::OwnedFd;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rustix::fs::{Mode, OFlags};
use rustix::io::{FdFlags, fcntl_getfd, fcntl_setfd};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{LocalModes, OptionalActions, Termios, tcgetattr, tcsetattr};

/// A pty with the attributes a new terminal has: the terminal's side, and
/// the program's.
pub fn pty() -> (File, OwnedFd) {
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
    // Not inherited by a child a test runs; set apart from the open, as
    // `posix_openpt` takes no `O_CLOEXEC` on macOS.
    fcntl_setfd(&master, fcntl_getfd(&master).unwrap() | FdFlags::CLOEXEC).unwrap();
    grantpt(&master).unwrap();
    unlockpt(&master).unwrap();
    let name = ptsname(&master, Vec::new()).unwrap();
    let slave = rustix::fs::open(
        name.as_c_str(),
        OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .unwrap();
    (File::from(master), slave)
}

/// A pty in raw mode, as a program reading keys sets it.
pub fn raw_pty() -> (File, OwnedFd) {
    let (master, slave) = pty();
    let mut termios = tcgetattr(&slave).unwrap();
    termios.make_raw();
    tcsetattr(&slave, OptionalActions::Now, &termios).unwrap();
    (master, slave)
}

/// Raw mode and the guard's record are one per process, so the tests that
/// touch them take turns.
pub fn terminal_state() -> MutexGuard<'static, ()> {
    static STATE: Mutex<()> = Mutex::new(());
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The local modes a program sets, without `PENDIN`: the BSDs and macOS set
/// that bit themselves when a tty goes back to canonical mode ("input to
/// retype"), so it says what the kernel did, not what was set.
pub fn local_modes(termios: &Termios) -> LocalModes {
    termios.local_modes - LocalModes::PENDIN
}
