//! The Unix side of the reader: the tty, `SIGWINCH` and the waker's pipe,
//! waited on together with `select`.

use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

use rustix::event::{FdSetElement, FdSetIter, Timespec, fd_set_insert, fd_set_num_elements};
use rustix::io::Errno;
use signal_hook::SigId;
use signal_hook::consts::SIGWINCH;

use super::Queue;
use crate::event::Event;

#[cfg(test)]
mod tests;

/// The most bytes taken from the tty in one read.
const READ_CHUNK: usize = 4096;

/// What a Unix reader waits on.
#[derive(Debug)]
pub struct Source {
    tty: OwnedFd,
    /// The read end of the pipe `SIGWINCH` writes to.
    winch: UnixStream,
    winch_id: SigId,
    wake: Arc<WakePipe>,
}

impl Source {
    /// Watches `tty`, with `SIGWINCH` watched from before this returns.
    pub fn new(tty: OwnedFd) -> io::Result<Self> {
        let (winch, winch_tx) = UnixStream::pair()?;
        winch.set_nonblocking(true)?;
        let (rx, tx) = UnixStream::pair()?;
        rx.set_nonblocking(true)?;
        tx.set_nonblocking(true)?;
        let winch_id = signal_hook::low_level::pipe::register(SIGWINCH, winch_tx)?;
        Ok(Self {
            tty,
            winch,
            winch_id,
            wake: Arc::new(WakePipe { rx, tx }),
        })
    }

    pub fn waker(&self) -> WakeHandle {
        WakeHandle(Arc::clone(&self.wake))
    }

    pub fn size(&self) -> io::Result<(u16, u16)> {
        let size = crate::terminal::window_size_of(&self.tty)?;
        Ok((size.columns, size.rows))
    }

    /// Waits up to `timeout` (with none, until something comes) and puts
    /// what came into `queue`: whether anything did. A signal ends the wait
    /// with nothing.
    pub fn wait(&mut self, timeout: Option<Duration>, queue: &mut Queue) -> io::Result<bool> {
        let ready = self.wait_ready(timeout)?;
        if ready.tty {
            self.read_tty(queue)?;
        }
        if ready.winch {
            drain(&self.winch)?;
            let (columns, rows) = self.size()?;
            queue.push(Event::Resize(columns, rows));
        }
        if ready.wake {
            drain(&self.wake.rx)?;
            queue.push(Event::Wake);
        }
        Ok(ready.tty || ready.winch || ready.wake)
    }

    /// Takes what the tty has into the queue.
    fn read_tty(&self, queue: &mut Queue) -> io::Result<()> {
        let mut buf = [0; READ_CHUNK];
        let n = match rustix::io::read(&self.tty, &mut buf) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the terminal closed",
                ));
            }
            Ok(n) => n,
            // The wait goes round again and reads it then. A non-blocking
            // fd (another library may have set O_NONBLOCK on stdin) says
            // so when what `select` saw is gone, read by someone else.
            Err(Errno::INTR | Errno::AGAIN) => return Ok(()),
            Err(err) => return Err(err.into()),
        };
        queue.feed(&buf[..n]);
        Ok(())
    }

    /// Which of the tty and the pipes have something to read.
    fn wait_ready(&self, timeout: Option<Duration>) -> io::Result<Ready> {
        // select, not poll: macOS's poll does not support devices, ttys
        // among them.
        let fds = [self.tty.as_fd(), self.winch.as_fd(), self.wake.rx.as_fd()];
        let nfds = fds.iter().map(AsRawFd::as_raw_fd).max().unwrap_or(0) + 1;
        let mut set = vec![FdSetElement::default(); fd_set_num_elements(fds.len(), nfds)];
        for fd in &fds {
            fd_set_insert(&mut set, fd.as_raw_fd());
        }
        // A wait too long for a timespec waits for ever.
        let timeout = timeout.and_then(|d| Timespec::try_from(d).ok());
        // SAFETY: every fd in the set is borrowed from `self`, so it stays
        // open for the whole call; the set has room for each, sized by
        // `fd_set_num_elements` for the highest.
        match unsafe { rustix::event::select(nfds, Some(&mut set), None, None, timeout.as_ref()) } {
            Ok(_) => {}
            Err(Errno::INTR) => return Ok(Ready::default()),
            Err(err) => return Err(err.into()),
        }
        let is_set = |fd: BorrowedFd<'_>| FdSetIter::new(&set).any(|r| r == fd.as_raw_fd());
        Ok(Ready {
            tty: is_set(fds[0]),
            winch: is_set(fds[1]),
            wake: is_set(fds[2]),
        })
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        // Also closes the pipe's write end, which signal-hook owns.
        signal_hook::low_level::unregister(self.winch_id);
    }
}

/// The waker's side of the pipe.
#[derive(Debug, Clone)]
pub struct WakeHandle(Arc<WakePipe>);

impl WakeHandle {
    pub fn wake(&self) -> io::Result<()> {
        loop {
            return match (&self.0.tx).write(&[0]) {
                // A full pipe already holds a wake.
                Ok(_) => Ok(()),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => Ok(()),
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => Err(err),
            };
        }
    }
}

/// The pipe a waker writes to and its reader waits on. Both ends live as
/// long as either side, so a write never meets a closed pipe (and its
/// `SIGPIPE`).
#[derive(Debug)]
struct WakePipe {
    rx: UnixStream,
    tx: UnixStream,
}

/// Which of the reader's fds have something to read.
#[derive(Debug, Default)]
struct Ready {
    tty: bool,
    winch: bool,
    wake: bool,
}

/// Reads a non-blocking pipe until it is empty.
fn drain(mut pipe: &UnixStream) -> io::Result<()> {
    let mut buf = [0; 64];
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
}
