//! The backend draws what ratatui-crossterm's `CrosstermBackend` draws: the
//! same buffers through both, byte for byte.

use ratatui_core::backend::{Backend, ClearType};
use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};
use ratatui_core::style::{Color, Modifier, Style};
use ratatui_crossterm::CrosstermBackend;

use super::TtyrBackend;

/// Draws what changed from `before` to `after` through `backend`, returning
/// the bytes it wrote.
fn draw_with<B: Backend>(backend: &mut B, before: &Buffer, after: &Buffer)
where
    B::Error: std::fmt::Debug,
{
    backend.draw(before.diff(after).into_iter()).unwrap();
}

/// The bytes both backends write for the change from `before` to `after`,
/// ttyr's then crossterm's.
fn both(before: &Buffer, after: &Buffer) -> (String, String) {
    let mut ttyr = TtyrBackend::new(Vec::new());
    draw_with(&mut ttyr, before, after);
    let mut crossterm = CrosstermBackend::new(Vec::new());
    draw_with(&mut crossterm, before, after);
    (
        String::from_utf8(ttyr.writer).unwrap(),
        String::from_utf8(crossterm.writer().clone()).unwrap(),
    )
}

fn area() -> Rect {
    Rect::new(0, 0, 24, 6)
}

/// Every colour ratatui has, as foreground, background and underline.
fn colours() -> Vec<Color> {
    let mut colours = vec![
        Color::Reset,
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::Gray,
        Color::DarkGray,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
        Color::White,
        Color::Rgb(1, 2, 3),
        Color::Indexed(200),
    ];
    colours.extend([Color::Rgb(255, 128, 0), Color::Indexed(16)]);
    colours
}

/// Every modifier ratatui has.
const MODIFIERS: [Modifier; 9] = [
    Modifier::BOLD,
    Modifier::DIM,
    Modifier::ITALIC,
    Modifier::UNDERLINED,
    Modifier::SLOW_BLINK,
    Modifier::RAPID_BLINK,
    Modifier::REVERSED,
    Modifier::HIDDEN,
    Modifier::CROSSED_OUT,
];

#[test]
fn plain_text_with_gaps_and_wide_characters() {
    let empty = Buffer::empty(area());
    let mut after = Buffer::empty(area());
    after.set_string(0, 0, "hello", Style::default());
    after.set_string(10, 0, "wide: 日本", Style::default());
    after.set_string(3, 4, "é😀x", Style::default());
    let (ttyr, crossterm) = both(&empty, &after);
    assert!(ttyr.contains("hello"));
    assert_eq!(ttyr, crossterm);
}

#[test]
fn every_colour_on_every_layer() {
    let empty = Buffer::empty(area());
    let mut after = Buffer::empty(area());
    for (i, &colour) in colours().iter().enumerate() {
        let i = u16::try_from(i).unwrap();
        let style = Style::default()
            .fg(colour)
            .bg(colours()[(usize::from(i) + 3) % colours().len()])
            .underline_color(colour);
        after.set_string(i % 24, i / 24, "x", style);
    }
    let (ttyr, crossterm) = both(&empty, &after);
    assert_eq!(ttyr, crossterm);
}

#[test]
fn every_change_between_modifiers() {
    let empty = Buffer::empty(area());
    let mut after = Buffer::empty(area());
    // Each modifier alone, each pair, and each pair's change to every
    // other, so every removal and addition is drawn.
    let mut styles = vec![Modifier::empty()];
    for a in MODIFIERS {
        styles.push(a);
        for b in MODIFIERS {
            styles.push(a | b);
        }
    }
    let width = area().width;
    for (i, modifier) in styles.iter().enumerate() {
        let i = u16::try_from(i).unwrap();
        let (x, y) = (i % width, (i / width) % area().height);
        after.set_string(x, y, "m", Style::default().add_modifier(*modifier));
    }
    let (ttyr, crossterm) = both(&empty, &after);
    assert_eq!(ttyr, crossterm);
}

#[test]
fn a_frame_drawn_over_another() {
    let mut before = Buffer::empty(area());
    before.set_string(0, 0, "one two three", Style::default().fg(Color::Red));
    let mut after = before.clone();
    after.set_string(4, 0, "TWO", Style::default().fg(Color::Blue).bold());
    after.set_string(0, 5, "status", Style::default().reversed());
    let (ttyr, crossterm) = both(&before, &after);
    assert_eq!(ttyr, crossterm);
}

#[test]
fn the_other_calls_write_crosstermbackends_bytes() {
    let mut ttyr = TtyrBackend::new(Vec::new());
    let mut crossterm = CrosstermBackend::new(Vec::new());
    for clear in [
        ClearType::All,
        ClearType::AfterCursor,
        ClearType::BeforeCursor,
        ClearType::CurrentLine,
        ClearType::UntilNewLine,
    ] {
        ttyr.clear_region(clear).unwrap();
        crossterm.clear_region(clear).unwrap();
    }
    ttyr.clear().unwrap();
    crossterm.clear().unwrap();
    ttyr.hide_cursor().unwrap();
    crossterm.hide_cursor().unwrap();
    ttyr.show_cursor().unwrap();
    crossterm.show_cursor().unwrap();
    ttyr.set_cursor_position((7, 3)).unwrap();
    crossterm.set_cursor_position((7, 3)).unwrap();
    ttyr.append_lines(3).unwrap();
    crossterm.append_lines(3).unwrap();
    assert_eq!(ttyr.writer, *crossterm.writer());
}

#[test]
fn the_cursor_position_is_where_the_backend_put_it() {
    let mut backend = TtyrBackend::new(Vec::new());
    assert_eq!(backend.get_cursor_position().unwrap(), Position::ORIGIN);
    backend.set_cursor_position((7, 3)).unwrap();
    assert_eq!(
        backend.get_cursor_position().unwrap(),
        Position { x: 7, y: 3 }
    );
    let empty = Buffer::empty(area());
    let mut after = Buffer::empty(area());
    after.set_string(2, 1, "ab", Style::default());
    draw_with(&mut backend, &empty, &after);
    assert_eq!(
        backend.get_cursor_position().unwrap(),
        Position { x: 4, y: 1 }
    );
    // Nothing was written in asking.
    let written = backend.writer.len();
    backend.get_cursor_position().unwrap();
    assert_eq!(backend.writer.len(), written);
}

#[test]
fn commands_go_through_the_backend_in_order() {
    let mut backend = TtyrBackend::new(Vec::new());
    crate::execute!(backend, crate::cursor::SetCursorStyle::SteadyBar).unwrap();
    backend.set_cursor_position((0, 0)).unwrap();
    assert_eq!(
        String::from_utf8(backend.writer).unwrap(),
        "\x1B[6 q\x1B[1;1H"
    );
}
