//! Ptys for the tests: the test writes to the terminal's side (the master),
//! as a terminal emulator would, and the code under test uses the program's.

use std::fs::File;
use std::os::fd::OwnedFd;

use rustix::fs::{Mode, OFlags};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{OptionalActions, tcgetattr, tcsetattr};

/// A pty with the attributes a new terminal has: the terminal's side, and
/// the program's.
pub fn pty() -> (File, OwnedFd) {
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
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
