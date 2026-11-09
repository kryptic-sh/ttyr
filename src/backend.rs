//! A ratatui backend on ttyr's commands: [`TtyrBackend`], in place of
//! ratatui-crossterm's `CrosstermBackend`. With the `ratatui` feature.
//!
//! It writes the bytes `CrosstermBackend` writes for the same buffers, so
//! switching backends changes nothing on screen. One difference:
//! `get_cursor_position` never asks the terminal (whose answer crossterm
//! waits for); it is where this backend last put the cursor.

use std::io::{self, Write};

use ratatui_core::backend::{Backend, ClearType, WindowSize};
use ratatui_core::buffer::Cell;
use ratatui_core::layout::{Position, Size};
use ratatui_core::style::{Color as RatatuiColor, Modifier};

use crate::cursor::{Hide, MoveTo, Show};
use crate::style::{
    Attribute, Color, Colors, Print, SetAttribute, SetBackgroundColor, SetColors,
    SetForegroundColor, SetUnderlineColor,
};
use crate::terminal::{self, Clear};
use crate::{execute, queue};

#[cfg(test)]
mod tests;

/// A ratatui backend writing to `W`, usually stdout.
///
/// It is also an [`io::Write`], so ttyr's commands queued into it between
/// frames (`execute!(terminal.backend_mut(), SetCursorStyle::SteadyBar)`)
/// go out in order with what it draws.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct TtyrBackend<W: Write> {
    writer: W,
    /// Where this backend last put the cursor.
    cursor: Position,
}

impl<W: Write> TtyrBackend<W> {
    /// A backend writing to `writer`.
    pub const fn new(writer: W) -> Self {
        Self {
            writer,
            cursor: Position::ORIGIN,
        }
    }

    /// The writer.
    pub const fn writer(&self) -> &W {
        &self.writer
    }

    /// The writer, to write to. What is written there is not in ratatui's
    /// idea of the screen, so the next frame may not draw over it.
    pub const fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }
}

impl<W: Write> Write for TtyrBackend<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl<W: Write> Backend for TtyrBackend<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut fg = RatatuiColor::Reset;
        let mut bg = RatatuiColor::Reset;
        let mut underline_color = RatatuiColor::Reset;
        let mut modifier = Modifier::empty();
        let mut last: Option<Position> = None;
        for (x, y, cell) in content {
            // Move only where the cell does not follow the last one.
            if !matches!(last, Some(p) if x == p.x + 1 && y == p.y) {
                queue!(self.writer, MoveTo(x, y))?;
            }
            last = Some(Position { x, y });
            if cell.modifier != modifier {
                queue_modifier_change(&mut self.writer, modifier, cell.modifier)?;
                modifier = cell.modifier;
            }
            if cell.fg != fg || cell.bg != bg {
                let colors = Colors::new(color(cell.fg), color(cell.bg));
                queue!(self.writer, SetColors(colors))?;
                fg = cell.fg;
                bg = cell.bg;
            }
            if cell.underline_color != underline_color {
                queue!(self.writer, SetUnderlineColor(color(cell.underline_color)))?;
                underline_color = cell.underline_color;
            }
            queue!(self.writer, Print(cell.symbol()))?;
        }
        if let Some(Position { x, y }) = last {
            self.cursor = Position {
                x: x.saturating_add(1),
                y,
            };
        }
        queue!(
            self.writer,
            SetForegroundColor(Color::Reset),
            SetBackgroundColor(Color::Reset),
            SetUnderlineColor(Color::Reset),
            SetAttribute(Attribute::Reset),
        )
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        execute!(self.writer, Hide)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        execute!(self.writer, Show)
    }

    /// Where this backend last put the cursor: by `set_cursor_position`, or
    /// one column after the start of the last cell it drew (a wide cell
    /// leaves it a column short). It does not ask the terminal, so it does
    /// not know of a move made by writing to the backend directly.
    fn get_cursor_position(&mut self) -> io::Result<Position> {
        Ok(self.cursor)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        let position = position.into();
        execute!(self.writer, MoveTo(position.x, position.y))?;
        self.cursor = position;
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        self.clear_region(ClearType::All)
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        let clear = match clear_type {
            ClearType::All => terminal::ClearType::All,
            ClearType::AfterCursor => terminal::ClearType::FromCursorDown,
            ClearType::BeforeCursor => terminal::ClearType::FromCursorUp,
            ClearType::CurrentLine => terminal::ClearType::CurrentLine,
            ClearType::UntilNewLine => terminal::ClearType::UntilNewLine,
        };
        execute!(self.writer, Clear(clear))
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        for _ in 0..n {
            queue!(self.writer, Print("\n"))?;
        }
        self.writer.flush()
    }

    fn size(&self) -> io::Result<Size> {
        let (width, height) = terminal::size()?;
        Ok(Size { width, height })
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        let size = terminal::window_size()?;
        Ok(WindowSize {
            columns_rows: Size {
                width: size.columns,
                height: size.rows,
            },
            pixels: Size {
                width: size.width,
                height: size.height,
            },
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// A ratatui colour as ttyr's, mapped as ratatui-crossterm maps it: ratatui's
/// plain colours are the dark half of the sixteen, its light ones the bright
/// half.
const fn color(color: RatatuiColor) -> Color {
    match color {
        RatatuiColor::Reset => Color::Reset,
        RatatuiColor::Black => Color::Black,
        RatatuiColor::Red => Color::DarkRed,
        RatatuiColor::Green => Color::DarkGreen,
        RatatuiColor::Yellow => Color::DarkYellow,
        RatatuiColor::Blue => Color::DarkBlue,
        RatatuiColor::Magenta => Color::DarkMagenta,
        RatatuiColor::Cyan => Color::DarkCyan,
        RatatuiColor::Gray => Color::Grey,
        RatatuiColor::DarkGray => Color::DarkGrey,
        RatatuiColor::LightRed => Color::Red,
        RatatuiColor::LightGreen => Color::Green,
        RatatuiColor::LightBlue => Color::Blue,
        RatatuiColor::LightYellow => Color::Yellow,
        RatatuiColor::LightMagenta => Color::Magenta,
        RatatuiColor::LightCyan => Color::Cyan,
        RatatuiColor::White => Color::White,
        RatatuiColor::Rgb(r, g, b) => Color::Rgb { r, g, b },
        RatatuiColor::Indexed(n) => Color::AnsiValue(n),
    }
}

/// Queues the attributes that turn `from` into `to`, in ratatui-crossterm's
/// order: what is removed first, then what is added. Bold and dim share one
/// off (normal intensity), after which whichever of them stays is set again.
fn queue_modifier_change(out: &mut impl Write, from: Modifier, to: Modifier) -> io::Result<()> {
    let removed = from - to;
    if removed.contains(Modifier::REVERSED) {
        queue!(out, SetAttribute(Attribute::NoReverse))?;
    }
    let reset_intensity = removed.intersects(Modifier::BOLD | Modifier::DIM);
    if reset_intensity {
        queue!(out, SetAttribute(Attribute::NormalIntensity))?;
        if to.contains(Modifier::DIM) {
            queue!(out, SetAttribute(Attribute::Dim))?;
        }
        if to.contains(Modifier::BOLD) {
            queue!(out, SetAttribute(Attribute::Bold))?;
        }
    }
    let offs = [
        (Modifier::ITALIC, Attribute::NoItalic),
        (Modifier::UNDERLINED, Attribute::NoUnderline),
        (Modifier::CROSSED_OUT, Attribute::NotCrossedOut),
        (Modifier::HIDDEN, Attribute::NoHidden),
    ];
    for (modifier, off) in offs {
        if removed.contains(modifier) {
            queue!(out, SetAttribute(off))?;
        }
    }
    if removed.intersects(Modifier::SLOW_BLINK | Modifier::RAPID_BLINK) {
        queue!(out, SetAttribute(Attribute::NoBlink))?;
    }
    let added = to - from;
    let ons = [
        (Modifier::REVERSED, Attribute::Reverse, true),
        (Modifier::BOLD, Attribute::Bold, !reset_intensity),
        (Modifier::ITALIC, Attribute::Italic, true),
        (Modifier::UNDERLINED, Attribute::Underlined, true),
        (Modifier::DIM, Attribute::Dim, !reset_intensity),
        (Modifier::CROSSED_OUT, Attribute::CrossedOut, true),
        (Modifier::HIDDEN, Attribute::Hidden, true),
        (Modifier::SLOW_BLINK, Attribute::SlowBlink, true),
        (Modifier::RAPID_BLINK, Attribute::RapidBlink, true),
    ];
    for (modifier, on, wanted) in ons {
        if wanted && added.contains(modifier) {
            queue!(out, SetAttribute(on))?;
        }
    }
    Ok(())
}
