//! A command's bytes, for the tests.

use crate::command::Command;

pub fn bytes(command: impl Command) -> String {
    let mut out = String::new();
    command.write_ansi(&mut out).unwrap();
    out
}
