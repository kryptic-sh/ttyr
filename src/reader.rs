//! Reading events from the terminal: [`Reader`], and the [`Waker`] another
//! thread wakes it with. On Unix it reads a tty; on Windows, the console.

use std::collections::VecDeque;
use std::io;
#[cfg(unix)]
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::event::{Decoder, Event, Report};

#[cfg(all(test, unix))]
mod tests;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as sys;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as sys;

/// Whether a [`Reader`] is open in this process.
static OPEN: AtomicBool = AtomicBool::new(false);

/// Events read from the terminal, one at a time, as the caller asks for
/// them.
///
/// It is the one reader of the terminal's input in the process: opening a
/// second while one is open fails with [`io::ErrorKind::ResourceBusy`],
/// instead of two readers racing for the same input. Dropping it lets the
/// next one open.
///
/// It reads only inside [`poll`](Self::poll) and [`read`](Self::read), never
/// in the background, so while the caller does not call them another program
/// can read the terminal.
///
/// Besides the terminal's input it delivers [`Event::Resize`] when the
/// terminal is resized (`SIGWINCH` on Unix) and [`Event::Wake`] when its
/// [`Waker`] is woken.
#[derive(Debug)]
pub struct Reader {
    source: sys::Source,
    queue: Queue,
    escape_timeout: Option<Duration>,
    // Last, so that it is released after everything else is closed.
    _claim: Claim,
}

/// What has been read and not yet delivered: events, and the bytes the
/// decoder holds.
#[derive(Debug)]
struct Queue {
    decoder: Decoder,
    /// Events decoded and not yet read, in the order they arrived.
    events: VecDeque<Event>,
    /// When the last bytes arrived, for the escape timeout.
    last_input: Instant,
    /// Whether the bytes the decoder holds have been flushed since they
    /// arrived, so that what is left (a longer sequence, a paste) waits for
    /// more input.
    flushed: bool,
}

impl Queue {
    /// Takes bytes the terminal sent, and the events they complete.
    fn feed(&mut self, bytes: &[u8]) {
        self.decoder.feed(bytes);
        self.last_input = Instant::now();
        self.flushed = false;
        while let Some(event) = self.decoder.next_event() {
            self.events.push_back(event);
        }
    }

    /// Takes an event that came whole.
    fn push(&mut self, event: Event) {
        self.events.push_back(event);
    }
}

impl Reader {
    /// A reader of `source`.
    fn new(source: sys::Source, claim: Claim) -> Self {
        Self {
            source,
            queue: Queue {
                decoder: Decoder::new(),
                events: VecDeque::new(),
                last_input: Instant::now(),
                flushed: false,
            },
            escape_timeout: None,
            _claim: claim,
        }
    }

    /// A reader of the controlling terminal, `/dev/tty`, whatever stdin is.
    ///
    /// `SIGWINCH` is watched from before this returns, so a resize after it
    /// returns is always delivered: a caller that has the size from before
    /// can compare it with [`size`](Self::size) once, and miss nothing.
    #[cfg(unix)]
    pub fn open() -> io::Result<Self> {
        Self::from_fd(crate::terminal::open_tty()?)
    }

    /// A reader of the process's console (`CONIN$`), whatever its standard
    /// handles are.
    ///
    /// Resize records are on from raw mode (`ENABLE_WINDOW_INPUT`), so a
    /// resize after raw mode is on and this returns is always delivered.
    #[cfg(windows)]
    pub fn open() -> io::Result<Self> {
        let claim = Claim::take()?;
        Ok(Self::new(sys::Source::open()?, claim))
    }

    /// A reader of `tty`, which should be a terminal: [`Event::Resize`] and
    /// [`size`](Self::size) read its size. `SIGWINCH` is watched from before
    /// this returns, as with [`open`](Self::open).
    #[cfg(unix)]
    pub fn from_fd(tty: OwnedFd) -> io::Result<Self> {
        let claim = Claim::take()?;
        Ok(Self::new(sys::Source::new(tty)?, claim))
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

    /// Says whether the kitty keyboard protocol is on, which decides what
    /// modifier bit 8 is: see [`Decoder::set_kitty_keyboard`]. A program that
    /// pushes kitty flags calls this, unless it reads the terminal's answer
    /// to a flags query after pushing them, which sets it too.
    pub fn set_kitty_keyboard(&mut self, on: bool) {
        self.queue.decoder.set_kitty_keyboard(on);
    }

    /// A handle that wakes this reader from another thread.
    pub fn waker(&self) -> Waker {
        Waker(self.source.waker())
    }

    /// The terminal's size, columns then rows.
    pub fn size(&self) -> io::Result<(u16, u16)> {
        self.source.size()
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
            if let Some(event) = self.queue.events.pop_front() {
                return Ok(event);
            }
            self.wait(None)?;
        }
    }

    /// Reads the answers to queries written before: until the answer to
    /// `RequestPrimaryDeviceAttributes` (DA1), which every terminal answers
    /// and in order, so write it last; or until `timeout` passes, for a
    /// terminal that does not answer it. Every event read meanwhile is kept
    /// in [`Answers::events`] in the order it came, the reports among keys
    /// typed, a resize or a wake.
    pub fn collect_answers(&mut self, timeout: Duration) -> io::Result<Answers> {
        let deadline = Instant::now().checked_add(timeout);
        let mut answers = Answers::default();
        while self.wait(deadline)? {
            let Some(event) = self.queue.events.pop_front() else {
                continue;
            };
            let da1 = matches!(event, Event::Report(Report::PrimaryDeviceAttributes(_)));
            answers.events.push(event);
            if da1 {
                answers.complete = true;
                break;
            }
        }
        Ok(answers)
    }

    /// Waits until an event is ready, `true`, or `deadline` passes, `false`;
    /// with no deadline, until an event is ready.
    fn wait(&mut self, deadline: Option<Instant>) -> io::Result<bool> {
        loop {
            let queue = &mut self.queue;
            if !queue.events.is_empty() {
                return Ok(true);
            }
            let now = Instant::now();
            // When to read what the decoder holds as typed: at once with no
            // escape timeout, or once the timeout passes with nothing more;
            // never, for a timeout too long to reach.
            let flush_at = if queue.decoder.has_pending() && !queue.flushed {
                self.escape_timeout
                    .map_or(Some(now), |t| queue.last_input.checked_add(t))
            } else {
                None
            };
            let until = match (deadline, flush_at) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            let timeout = until.map(|u| u.saturating_duration_since(now));
            if self.source.wait(timeout, queue)? {
                continue;
            }
            let now = Instant::now();
            if flush_at.is_some_and(|at| now >= at) {
                while let Some(event) = queue.decoder.flush_event() {
                    queue.events.push_back(event);
                }
                queue.flushed = true;
            } else if deadline.is_some_and(|d| now >= d) {
                return Ok(!queue.events.is_empty());
            }
        }
    }
}

/// What [`Reader::collect_answers`] read.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Answers {
    /// Every event, in the order it came: the reports, DA1's last when it
    /// came, among anything else read meanwhile.
    pub events: Vec<Event>,
    /// Whether DA1's answer came: if not, the time ran out first, and the
    /// answers still to come arrive later as events.
    pub complete: bool,
}

impl Answers {
    /// The reports, in the order they came.
    pub fn reports(&self) -> impl Iterator<Item = &Report> {
        self.events.iter().filter_map(|event| match event {
            Event::Report(report) => Some(report),
            _ => None,
        })
    }

    /// The events that are not reports, in the order they came.
    pub fn others(&self) -> impl Iterator<Item = &Event> {
        self.events
            .iter()
            .filter(|event| !matches!(event, Event::Report(_)))
    }
}

/// Wakes a [`Reader`] from another thread: its wait ends and it delivers
/// [`Event::Wake`]. Wakes before the reader next waits are kept, several of
/// them as one event. Waking after the reader is dropped does nothing.
#[derive(Debug, Clone)]
pub struct Waker(sys::WakeHandle);

impl Waker {
    /// Wakes the reader.
    pub fn wake(&self) -> io::Result<()> {
        self.0.wake()
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
