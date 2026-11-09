# 03 — Reading a Unix tty

- **Blocking API**: `poll(timeout) -> bool` and `read() -> Event`, as crossterm
  (the whole of hjkl's use, and tmxr's).
- **The source**: the controlling tty (`/dev/tty`) or a given fd, so a program
  whose stdin is a pipe still reads the terminal.
- **Waking**: poll on the tty and a self-pipe, so `SIGWINCH` (and a caller's
  wake-up) interrupts a wait. A SIGWINCH that arrives with input keeps both
  (tmxr's fork).
- **Reading**: drain what is available into the decoder, deliver events a read
  at a time; never block mid-sequence. The Escape timeout is applied here: the
  reader calls `Decoder::flush_event` once the timeout passes with nothing more
  read, or as soon as nothing more is waiting when the timeout is none (tmxr's
  client, which applies `escape-time` in its server).
- **Poll semantics hjkl's loop relies on**: timers (which-key, chord timeout,
  hover, an idle hint) run off `poll(timeout)`, so a timed-out poll returns
  false promptly, and `EINTR` is retried, never surfaced as an error. hjkl
  drains with `while poll(Duration::ZERO)` after each event, so a zero-timeout
  poll is cheap (no syscall when events are already decoded) and never reports
  an event while only part of an escape sequence has arrived.
- **A cross-thread waker**: the self-pipe exposed as a handle another thread can
  wake the loop with (hjkl's LSP and file-watch workers), with its own event,
  instead of the loop polling on a timeout to notice them.
- **Async** (feature-gated, runtime-neutral): a `Stream` of events over the
  non-blocking fd. hjkl does not need it today; the shape is an open question
  in 01.
- **Threads**: the reader is one owner of the tty's input; a second reader is a
  programming error that fails loudly, not two readers racing (crossterm's
  internal reader and a caller's competing is how a blocking probe hangs).
- **Tests**: a pty pair drives the reader end to end (partial sequences across
  reads, SIGWINCH during a wait, long bursts).
