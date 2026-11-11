//! The reader tests that read the same on every platform, written once over
//! a [`Terminal`]: a pty on Unix, the console with VT input on Windows. Each
//! platform's tests stamp them out with [`shared_tests!`].

use std::thread;
use std::time::{Duration, Instant};

use super::Reader;
use crate::event::{Event, KeyCode, KeyEncoding, KeyEvent, KeyModifiers, ModeSetting, Report};

/// Longer than any wait a test expects to end, so a hang fails instead.
pub const LONG: Duration = Duration::from_secs(5);

/// A terminal a test types into, as a person or the terminal itself would,
/// held for the test's turn at the one reader and terminal per process.
pub trait Terminal {
    /// The reader of this terminal.
    fn reader(&mut self) -> Reader;
    /// Sends `text` as the terminal sends bytes.
    fn type_text(&mut self, text: &str);
    /// [`Terminal::type_text`] from another thread.
    fn typist(&self) -> impl FnMut(&str) + Send + 'static;
}

/// A key as a legacy terminal's bytes give it.
pub fn legacy(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE).with_encoding(KeyEncoding::Legacy))
}

/// A `#[test]` for each shared test, on the terminal `$terminal` makes.
macro_rules! shared_tests {
    ($terminal:expr) => {
        $crate::reader::shared_tests::shared_tests!(
            @ $terminal;
            a_timed_out_poll_returns_false_promptly,
            a_zero_poll_reports_an_event_already_decoded,
            with_no_escape_timeout_a_lone_escape_is_read_at_once,
            an_escape_timeout_keeps_a_split_sequence_whole,
            an_escape_timeout_reads_a_lone_escape_once_it_passes,
            an_escape_timeout_too_long_to_reach_never_flushes,
            a_paste_waits_past_a_flush_for_its_end,
            a_waker_wakes_a_wait_from_another_thread,
            a_waker_outliving_its_reader_does_nothing,
            collect_answers_gives_up_at_the_timeout,
            collect_answers_returns_a_wake_that_comes_while_it_waits,
            wakes_that_fill_the_pipe_are_one_event,
            the_kitty_protocol_decides_what_bit_8_is,
        );
    };
    (@ $terminal:expr; $($name:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                $crate::reader::shared_tests::$name($terminal);
            }
        )*
    };
}
pub(crate) use shared_tests;

pub fn a_timed_out_poll_returns_false_promptly(mut terminal: impl Terminal) {
    let mut reader = terminal.reader();
    let wait = Duration::from_millis(50);
    let start = Instant::now();
    assert!(!reader.poll(wait).unwrap());
    let took = start.elapsed();
    assert!(took >= wait && took < LONG, "{took:?}");
    assert!(!reader.poll(Duration::ZERO).unwrap());
}

pub fn a_zero_poll_reports_an_event_already_decoded(mut terminal: impl Terminal) {
    let mut reader = terminal.reader();
    terminal.type_text("ab");
    assert!(reader.poll(LONG).unwrap());
    // Both keys arrived in one read: the second is ready without a wait.
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Char('a')));
    assert!(reader.poll(Duration::ZERO).unwrap());
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Char('b')));
}

pub fn with_no_escape_timeout_a_lone_escape_is_read_at_once(mut terminal: impl Terminal) {
    let mut reader = terminal.reader();
    terminal.type_text("\x1B");
    assert!(reader.poll(LONG).unwrap());
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Esc));
}

pub fn an_escape_timeout_keeps_a_split_sequence_whole(mut terminal: impl Terminal) {
    let mut reader = terminal.reader();
    reader.set_escape_timeout(Some(LONG));
    terminal.type_text("\x1B");
    // The ESC arrives and is held, not read as Escape.
    assert!(!reader.poll(Duration::from_millis(100)).unwrap());
    assert!(!reader.poll(Duration::ZERO).unwrap());
    terminal.type_text("[A");
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Up));
}

pub fn an_escape_timeout_reads_a_lone_escape_once_it_passes(mut terminal: impl Terminal) {
    let mut reader = terminal.reader();
    let timeout = Duration::from_millis(100);
    reader.set_escape_timeout(Some(timeout));
    let start = Instant::now();
    terminal.type_text("\x1B");
    assert!(reader.poll(LONG).unwrap());
    assert!(start.elapsed() >= timeout);
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Esc));
}

pub fn an_escape_timeout_too_long_to_reach_never_flushes(mut terminal: impl Terminal) {
    let mut reader = terminal.reader();
    reader.set_escape_timeout(Some(Duration::MAX));
    terminal.type_text("\x1B");
    assert!(!reader.poll(Duration::from_millis(100)).unwrap());
    terminal.type_text("[A");
    assert_eq!(reader.read().unwrap(), legacy(KeyCode::Up));
}

pub fn a_paste_waits_past_a_flush_for_its_end(mut terminal: impl Terminal) {
    let mut reader = terminal.reader();
    terminal.type_text("\x1B[200~half");
    // The flush leaves the paste held, and the wait does not spin on it.
    assert!(!reader.poll(Duration::from_millis(100)).unwrap());
    terminal.type_text(" and half\x1B[201~");
    assert_eq!(reader.read().unwrap(), Event::Paste("half and half".into()));
}

pub fn a_waker_wakes_a_wait_from_another_thread(mut terminal: impl Terminal) {
    let mut reader = terminal.reader();
    let waker = reader.waker();
    let waking = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        waker.wake().unwrap();
        waker
    });
    let start = Instant::now();
    assert_eq!(reader.read().unwrap(), Event::Wake);
    assert!(start.elapsed() < LONG);
    let waker = waking.join().unwrap();
    assert!(!reader.poll(Duration::ZERO).unwrap());
    // Wakes before the reader next waits are one event.
    waker.wake().unwrap();
    waker.wake().unwrap();
    assert!(reader.poll(Duration::ZERO).unwrap());
    assert_eq!(reader.read().unwrap(), Event::Wake);
    assert!(!reader.poll(Duration::ZERO).unwrap());
}

pub fn a_waker_outliving_its_reader_does_nothing(mut terminal: impl Terminal) {
    let reader = terminal.reader();
    let waker = reader.waker();
    drop(reader);
    waker.wake().unwrap();
}

pub fn collect_answers_gives_up_at_the_timeout(mut terminal: impl Terminal) {
    let mut reader = terminal.reader();
    terminal.type_text("\x1B[?2026;2$yq");
    let timeout = Duration::from_millis(100);
    let start = Instant::now();
    let answers = reader.collect_answers(timeout).unwrap();
    assert!(start.elapsed() >= timeout && start.elapsed() < LONG);
    assert!(!answers.complete);
    let mode = Report::Mode {
        mode: 2026,
        private: true,
        setting: ModeSetting::Reset,
    };
    assert_eq!(answers.reports().collect::<Vec<_>>(), [&mode]);
    assert_eq!(
        answers.events,
        [Event::Report(mode.clone()), legacy(KeyCode::Char('q'))]
    );
}

pub fn collect_answers_returns_a_wake_that_comes_while_it_waits(mut terminal: impl Terminal) {
    let mut reader = terminal.reader();
    let waker = reader.waker();
    let mut typist = terminal.typist();
    let answering = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        waker.wake().unwrap();
        thread::sleep(Duration::from_millis(50));
        typist("\x1B[?62c");
    });
    let answers = reader.collect_answers(LONG).unwrap();
    answering.join().unwrap();
    // The wake did not end the wait: DA1's answer came, last.
    assert!(answers.complete);
    let da1 = Event::Report(Report::PrimaryDeviceAttributes(vec![62]));
    assert_eq!(answers.events.last(), Some(&da1));
    // The wake came first, unless a loaded machine delayed the reader past
    // both, when the one wait reads the answer first and leaves the wake.
    if answers.events.len() == 2 {
        assert_eq!(answers.events[0], Event::Wake);
    } else {
        assert_eq!(answers.events, [da1]);
        assert_eq!(reader.read().unwrap(), Event::Wake);
    }
}

pub fn wakes_that_fill_the_pipe_are_one_event(mut terminal: impl Terminal) {
    let mut reader = terminal.reader();
    let waker = reader.waker();
    // Far more than a pipe holds (Unix): a full pipe already holds a wake.
    // An event (Windows) holds one wake however many come.
    for _ in 0..100_000 {
        waker.wake().unwrap();
    }
    assert_eq!(reader.read().unwrap(), Event::Wake);
    assert!(!reader.poll(Duration::ZERO).unwrap());
}

pub fn the_kitty_protocol_decides_what_bit_8_is(mut terminal: impl Terminal) {
    let mut reader = terminal.reader();
    let up = |modifiers| {
        Event::Key(KeyEvent::new(KeyCode::Up, modifiers).with_encoding(KeyEncoding::Legacy))
    };
    terminal.type_text("\x1B[1;9A");
    assert_eq!(reader.read().unwrap(), up(KeyModifiers::META));
    reader.set_kitty_keyboard(true);
    terminal.type_text("\x1B[1;9A");
    assert_eq!(reader.read().unwrap(), up(KeyModifiers::SUPER));
}
