//! The command traits and macros.

use std::fmt;
use std::io::{self, Write};

use super::{Command, ExecutableCommand, QueueableCommand};

/// A command writing its text in two pieces.
struct Text(&'static str, &'static str);

impl Command for Text {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str(self.0)?;
        f.write_str(self.1)
    }
}

/// A writer that records what reached it and how often it was flushed.
#[derive(Default)]
struct Recorder {
    bytes: Vec<u8>,
    flushes: usize,
}

impl Write for Recorder {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

/// A writer that refuses everything.
struct Broken;

impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn queue_writes_in_order_without_flushing() {
    let mut out = Recorder::default();
    crate::queue!(out, Text("a", "b"), Text("c", "d")).unwrap();
    assert_eq!(out.bytes, b"abcd");
    assert_eq!(out.flushes, 0);
}

#[test]
fn execute_writes_and_flushes_once() {
    let mut out = Recorder::default();
    crate::execute!(out, Text("a", "b"), &Text("c", "d")).unwrap();
    assert_eq!(out.bytes, b"abcd");
    assert_eq!(out.flushes, 1);
}

#[test]
fn the_macros_take_a_writer_by_mut_reference_and_as_a_temporary() {
    fn through(out: &mut Recorder) -> io::Result<()> {
        crate::execute!(out, Text("x", ""))
    }
    let mut out = Recorder::default();
    through(&mut out).unwrap();
    assert_eq!(out.bytes, b"x");
    crate::execute!(io::sink(), Text("y", "")).unwrap();
}

#[test]
fn the_writers_error_comes_back_and_stops_the_rest() {
    let mut evaluated = false;
    let err = crate::queue!(Broken, Text("a", ""), {
        evaluated = true;
        Text("b", "")
    })
    .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    assert!(!evaluated);
}

#[test]
fn the_traits_chain() {
    let mut out = Recorder::default();
    out.queue(Text("a", ""))
        .unwrap()
        .execute(Text("b", ""))
        .unwrap();
    assert_eq!(out.bytes, b"ab");
    assert_eq!(out.flushes, 1);
}
