//! A command's bytes, and a writer that fails, for the tests.

use std::io;

use crate::command::Command;

pub fn bytes(command: impl Command) -> String {
    let mut out = String::new();
    command.write_ansi(&mut out).unwrap();
    out
}

/// A writer that refuses everything, as a terminal that is gone does.
pub struct Broken;

impl io::Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
