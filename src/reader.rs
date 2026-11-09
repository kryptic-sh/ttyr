//! Reading events from a Unix tty: [`Reader`], and the [`Waker`] another
//! thread wakes it with.
//!
//! Unix only for now; the Windows console's reader is planned (phase 07 in
//! `docs/plan/`), so on Windows this module does not exist and a use of it
//! fails to compile.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rustix::event::{FdSetElement, FdSetIter, Timespec, fd_set_insert, fd_set_num_elements};
use rustix::io::Errno;
use signal_hook::SigId;
use signal_hook::consts::SIGWINCH;

use crate::event::{Decoder, Event};

#[cfg(test)]
mod tests;

/// The most bytes taken from the tty in one read.
const READ_CHUNK: usize = 4096;

/// Whether a [`Reader`] is open in this process.
static OPEN: AtomicBool = AtomicBool::new(false);

/// Events read from a tty, one at a time, as the caller asks for them.
///
/// It is the one reader of the tty's input in the process: opening a second
/// while one is open fails with [`io::ErrorKind::ResourceBusy`], instead of
/// two readers racing for the same bytes. Dropping it lets the next one open.
///
/// It reads only inside [`poll`](Self::poll) and [`read`](Self::read), never
/// in the background, so while the caller does not call them another program
/// can read the tty.
///
/// Besides the terminal's input it delivers [`Event::Resize`] on `SIGWINCH`
/// and [`Event::Wake`] when its [`Waker`] is woken.
#[derive(Debug)]
pub struct Reader {
    tty: OwnedFd,
    decoder: Decoder,
    /// Events decoded and not yet read, in the order they arrived.
    events: VecDeque<Event>,
    escape_timeout: Option<Duration>,
    /// When the tty's last bytes arrived, for the escape timeout.
    last_input: Instant,
    /// Whether the bytes the decoder holds have been flushed since they
    /// arrived, so that what is left (a longer sequence, a paste) waits for
    /// more input.
    flushed: bool,
    /// The read end of the pipe `SIGWINCH` writes to.
    winch: UnixStream,
    winch_id: SigId,
    wake: Arc<WakePipe>,
    // Last, so that it is released after everything else is closed.
    _claim: Claim,
}

impl Reader {
    /// A reader of the controlling terminal, `/dev/tty`, whatever stdin is.
    ///
    /// `SIGWINCH` is watched from before this returns, so a resize after it
    /// returns is always delivered: a caller that has the size from before
    /// can compare it with [`size`](Self::size) once, and miss nothing.
    pub fn open() -> io::Result<Self> {
        Self::from_fd(crate::terminal::open_tty()?)
    }

    /// A reader of `tty`, which should be a terminal: [`Event::Resize`] and
    /// [`size`](Self::size) read its size. `SIGWINCH` is watched from before
    /// this returns, as with [`open`](Self::open).
    pub fn from_fd(tty: OwnedFd) -> io::Result<Self> {
        let claim = Claim::take()?;
        let (winch, winch_tx) = UnixStream::pair()?;
        winch.set_nonblocking(true)?;
        let (rx, tx) = UnixStream::pair()?;
        rx.set_nonblocking(true)?;
        tx.set_nonblocking(true)?;
        let winch_id = signal_hook::low_level::pipe::register(SIGWINCH, winch_tx)?;
        Ok(Self {
            tty,
            decoder: Decoder::new(),
            events: VecDeque::new(),
            escape_timeout: None,
            last_input: Instant::now(),
            flushed: false,
            winch,
            winch_id,
            wake: Arc::new(WakePipe { rx, tx }),
            _claim: claim,
        })
    }

    /// How long a lone `ESC` (or a sequence cut short) waits for the rest of
    /// a sequence before it is read as typed (see
    /// [`Decoder::flush_event`]).
    ///
    /// `None`, the default, waits for nothing: once no more input is waiting
    /// it is read as typed at once, as crossterm does. A sequence split across
    /// the terminal's writes can then arrive as Escape and typing; a timeout
    /// keeps it whole, at the cost of that delay to a lone Escape.
    pub fn set_escape_timeout(&mut self, timeout: Option<Duration>) {
        self.escape_timeout = timeout;
    }

    /// A handle that wakes this reader from another thread.
    pub fn waker(&self) -> Waker {
        Waker(Arc::clone(&self.wake))
    }

    /// The terminal's size, columns then rows.
    pub fn size(&self) -> io::Result<(u16, u16)> {
        let size = crate::terminal::window_size_of(&self.tty)?;
        Ok((size.columns, size.rows))
    }

    /// Whether an event is ready to [`read`](Self::read), waiting up to
    /// `timeout` for one. An event already decoded returns at once with no
    /// system call, and a zero `timeout` never waits. A signal interrupting
    /// the wait does not end it early.
    pub fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        self.wait(Instant::now().checked_add(timeout))
    }

    /// The next event, waiting for one as long as it takes.
    pub fn read(&mut self) -> io::Result<Event> {
        loop {
            if let Some(event) = self.events.pop_front() {
                return Ok(event);
            }
            self.wait(None)?;
        }
    }

    /// Waits until an event is ready, `true`, or `deadline` passes, `false`;
    /// with no deadline, until an event is ready.
    fn wait(&mut self, deadline: Option<Instant>) -> io::Result<bool> {
        loop {
            if !self.events.is_empty() {
                return Ok(true);
            }
            let now = Instant::now();
            // When to read what the decoder holds as typed: at once with no
            // escape timeout, or once the timeout passes with nothing more.
            let flush_at = (self.decoder.has_pending() && !self.flushed)
                .then(|| self.escape_timeout.map_or(now, |t| self.last_input + t));
            let until = match (deadline, flush_at) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            let ready = self.wait_ready(until.map(|u| u.saturating_duration_since(now)))?;
            if ready.tty {
                self.read_tty()?;
            }
            if ready.winch {
                drain(&self.winch)?;
                let (columns, rows) = self.size()?;
                self.events.push_back(Event::Resize(columns, rows));
            }
            if ready.wake {
                drain(&self.wake.rx)?;
                self.events.push_back(Event::Wake);
            }
            if ready.any() {
                continue;
            }
            let now = Instant::now();
            if flush_at.is_some_and(|at| now >= at) {
                while let Some(event) = self.decoder.flush_event() {
                    self.events.push_back(event);
                }
                self.flushed = true;
            } else if deadline.is_some_and(|d| now >= d) {
                return Ok(!self.events.is_empty());
            }
        }
    }

    /// Takes what the tty has into the decoder, and the events it completes.
    fn read_tty(&mut self) -> io::Result<()> {
        let mut buf = [0; READ_CHUNK];
        let n = match rustix::io::read(&self.tty, &mut buf) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the terminal closed",
                ));
            }
            Ok(n) => n,
            // The wait goes round again and reads it then.
            Err(Errno::INTR) => return Ok(()),
            Err(err) => return Err(err.into()),
        };
        self.decoder.feed(&buf[..n]);
        self.last_input = Instant::now();
        self.flushed = false;
        while let Some(event) = self.decoder.next_event() {
            self.events.push_back(event);
        }
        Ok(())
    }

    /// Which of the tty and the pipes have something to read, waiting up to
    /// `timeout` (with none, until one does). A signal ends the wait with
    /// nothing ready.
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

impl Drop for Reader {
    fn drop(&mut self) {
        // Also closes the pipe's write end, which signal-hook owns.
        signal_hook::low_level::unregister(self.winch_id);
    }
}

/// Wakes a [`Reader`] from another thread: its wait ends and it delivers
/// [`Event::Wake`]. Wakes before the reader next waits are kept, several of
/// them as one event. Waking after the reader is dropped does nothing.
#[derive(Debug, Clone)]
pub struct Waker(Arc<WakePipe>);

impl Waker {
    /// Wakes the reader.
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

/// The pipe a [`Waker`] writes to and its [`Reader`] waits on. Both ends live
/// as long as either side, so a write never meets a closed pipe (and its
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

impl Ready {
    const fn any(&self) -> bool {
        self.tty || self.winch || self.wake
    }
}

/// The one open [`Reader`]'s hold on [`OPEN`], released when dropped.
#[derive(Debug)]
struct Claim;

impl Claim {
    fn take() -> io::Result<Self> {
        OPEN.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| Self)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::ResourceBusy,
                    "a ttyr Reader is already open in this process",
                )
            })
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        OPEN.store(false, Ordering::Release);
    }
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
