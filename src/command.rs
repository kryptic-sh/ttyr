//! Commands: what a program writes to the terminal, as ANSI/xterm bytes,
//! queued into any [`io::Write`] with [`queue!`](crate::queue) and flushed
//! with [`execute!`](crate::execute), crossterm's shape.

use std::{fmt, io};

#[cfg(test)]
mod tests;

/// Something written to the terminal as ANSI/xterm bytes.
pub trait Command {
    /// Writes the command's bytes into `f`.
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result;
}

impl<T: Command> Command for &T {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        (**self).write_ansi(f)
    }
}

/// Queues commands into a writer, unflushed.
pub trait QueueableCommand {
    /// Writes `command`'s bytes, without flushing.
    fn queue(&mut self, command: impl Command) -> io::Result<&mut Self>;
}

/// Writes commands into a writer and flushes it.
pub trait ExecutableCommand {
    /// Writes `command`'s bytes and flushes.
    fn execute(&mut self, command: impl Command) -> io::Result<&mut Self>;
}

impl<W: io::Write + ?Sized> QueueableCommand for W {
    fn queue(&mut self, command: impl Command) -> io::Result<&mut Self> {
        write_command(self, &command)?;
        Ok(self)
    }
}

impl<W: io::Write + ?Sized> ExecutableCommand for W {
    fn execute(&mut self, command: impl Command) -> io::Result<&mut Self> {
        self.queue(command)?;
        self.flush()?;
        Ok(self)
    }
}

/// Writes `command` into `out`, keeping the writer's own error.
fn write_command(out: &mut (impl io::Write + ?Sized), command: &impl Command) -> io::Result<()> {
    /// `out` as a `fmt::Write`, holding on to the `io::Error` that a
    /// `fmt::Error` cannot carry.
    struct Adapter<'a, W: ?Sized> {
        out: &'a mut W,
        error: Option<io::Error>,
    }

    impl<W: io::Write + ?Sized> fmt::Write for Adapter<'_, W> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            self.out.write_all(s.as_bytes()).map_err(|err| {
                self.error = Some(err);
                fmt::Error
            })
        }
    }

    let mut adapter = Adapter { out, error: None };
    command.write_ansi(&mut adapter).map_err(|fmt::Error| {
        adapter
            .error
            .take()
            .unwrap_or_else(|| io::Error::other("a command failed to format"))
    })
}

/// Queues commands into a writer without flushing it:
/// `queue!(out, EnterAlternateScreen, EnableBracketedPaste)`. `out` is any
/// [`io::Write`], by value or by `&mut`; the commands after one that fails
/// are not written. Evaluates to `io::Result<()>`.
#[macro_export]
macro_rules! queue {
    ($writer:expr $(, $command:expr)* $(,)?) => {{
        // For `$writer.by_ref()` below, the method form that takes a writer
        // by value, by `&mut`, or as a binding not declared `mut`; redundant
        // where the caller has `Write` in scope already.
        #[allow(unused_imports)]
        use ::std::io::Write as _;
        // A match keeps a temporary writer (`stdout()`) alive to its end.
        match $writer.by_ref() {
            writer => {
                let result: ::std::io::Result<()> = Ok(());
                $(
                    let result = result.and_then(|()| {
                        $crate::command::QueueableCommand::queue(&mut *writer, $command)
                            .map(|_| ())
                    });
                )*
                result
            }
        }
    }};
}

/// Writes commands into a writer and flushes it, as [`queue!`] then a flush.
/// Evaluates to `io::Result<()>`.
#[macro_export]
macro_rules! execute {
    ($writer:expr $(, $command:expr)* $(,)?) => {{
        // For `$writer.by_ref()` below, the method form that takes a writer
        // by value, by `&mut`, or as a binding not declared `mut`; redundant
        // where the caller has `Write` in scope already.
        #[allow(unused_imports)]
        use ::std::io::Write as _;
        match $writer.by_ref() {
            // Named in full: a writer that is also a ratatui backend has a
            // `flush` of its own.
            writer => $crate::queue!(writer $(, $command)*)
                .and_then(|()| ::std::io::Write::flush(writer)),
        }
    }};
}

/// Defines unit commands that each write one fixed sequence:
/// `fixed_commands! { /// Doc. Name => "bytes"; }`.
macro_rules! fixed_commands {
    ($($(#[$doc:meta])* $name:ident => $bytes:literal;)*) => {$(
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name;

        impl $crate::command::Command for $name {
            fn write_ansi(&self, f: &mut impl ::std::fmt::Write) -> ::std::fmt::Result {
                f.write_str($bytes)
            }
        }
    )*};
}

pub(crate) use fixed_commands;
