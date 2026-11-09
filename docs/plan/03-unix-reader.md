# 03 — Reading a Unix tty

- **Blocking API**: `poll(timeout) -> bool` and `read() -> Event`, as crossterm
  (the whole of hjkl's use, and tmxr's).
- **The source**: the controlling tty (`/dev/tty`) or a given fd, so a program
  whose stdin is a pipe still reads the terminal.
- **Waking**: poll on the tty and a self-pipe, so `SIGWINCH` (and a caller's
  wake-up) interrupts a wait. A SIGWINCH that arrives with input keeps both
  (tmxr's fork).
- **Reading**: drain what is available into the decoder, deliver events a read
  at a time; never block mid-sequence. The Escape timeout from 02 is applied
  here.
- **Async** (feature-gated, runtime-neutral): a `Stream` of events over the
  non-blocking fd. hjkl does not need it today; the shape is an open question
  in 01.
- **Threads**: the reader is one owner of the tty's input; a second reader is a
  programming error that fails loudly, not two readers racing (crossterm's
  internal reader and a caller's competing is how a blocking probe hangs).
- **Tests**: a pty pair drives the reader end to end (partial sequences across
  reads, SIGWINCH during a wait, long bursts).
