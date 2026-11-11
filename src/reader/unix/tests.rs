//! The Unix source's reads, below the reader.

use std::collections::VecDeque;
use std::time::Instant;

use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};

use super::Source;
use crate::event::Decoder;
use crate::reader::Queue;
use crate::test_pty::raw_pty;

fn queue() -> Queue {
    Queue {
        decoder: Decoder::new(),
        events: VecDeque::new(),
        last_input: Instant::now(),
        flushed: false,
    }
}

#[test]
fn a_non_blocking_tty_with_nothing_to_read_reads_nothing() {
    // Another library may have set O_NONBLOCK on the fd, and what select
    // saw can be gone by the read.
    let (_term, tty) = raw_pty();
    fcntl_setfl(&tty, fcntl_getfl(&tty).unwrap() | OFlags::NONBLOCK).unwrap();
    let source = Source::new(tty).unwrap();
    let mut queue = queue();
    source.read_tty(&mut queue).unwrap();
    assert!(queue.events.is_empty());
    assert!(!queue.decoder.has_pending());
}
