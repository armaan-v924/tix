//! Log output for the `tix` binary.
//!
//! Two shapes, picked by the resolved level:
//!
//! - **Compact** (`info` and quieter, the default): one line per event, just
//!   the message and its fields. Warnings and errors are prefixed
//!   `warning:` / `error:`, matching the `error: <message>` a failed command
//!   exits with. No timestamp, no level column, no module path: what a person
//!   running a command wants to read.
//! - **Detailed** (`debug` and `trace`, i.e. `-v` or `--log-level debug`):
//!   tracing's full format, with timestamps, levels, and the emitting module,
//!   for working out what tix actually did.
//!
//! Both go to stderr, so stdout carries only results and stays pipeable.
//! Colour is used only when stderr is a terminal and `NO_COLOR` is unset.

use std::fmt;
use std::io::IsTerminal;

use tracing::{Event, Level, Subscriber};
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::registry::LookupSpan;

/// Installs the global subscriber for `level`. Call once, before any event.
pub fn init(level: Level) {
    let ansi = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let builder = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .with_ansi(ansi);
    if detailed(level) {
        builder.init();
    } else {
        builder.event_format(Compact).init();
    }
}

/// True when `level` asks for the detailed format: anything more verbose
/// than `info`. (tracing orders levels by verbosity, `TRACE` greatest.)
pub fn detailed(level: Level) -> bool {
    level > Level::INFO
}

/// The default event format: `[label: ]message key=value…`.
pub struct Compact;

impl<S, N> FormatEvent<S, N> for Compact
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        if let Some((label, color)) = label(*event.metadata().level()) {
            if writer.has_ansi_escapes() {
                write!(writer, "\x1b[1;{color}m{label}:\x1b[0m ")?;
            } else {
                write!(writer, "{label}: ")?;
            }
        }
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

/// The prefix and ANSI colour for a level; `info` lines carry none, since
/// they are the ordinary status a command reports.
fn label(level: Level) -> Option<(&'static str, u8)> {
    match level {
        Level::ERROR => Some(("error", 31)),
        Level::WARN => Some(("warning", 33)),
        Level::INFO => None,
        // Unreachable under the compact format's own levels, but labelled in
        // case it is ever paired with a more verbose filter.
        Level::DEBUG => Some(("debug", 34)),
        Level::TRACE => Some(("trace", 35)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Runs `emit` under a compact subscriber at `level`, returning what it
    /// wrote.
    fn capture(level: Level, emit: impl FnOnce()) -> String {
        let buffer = Buffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(level)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .event_format(Compact)
            .finish();
        tracing::subscriber::with_default(subscriber, emit);
        String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap()
    }

    #[test]
    fn info_is_bare_message_and_fields() {
        let out = capture(Level::INFO, || {
            tracing::info!(branch = "feat/x", "created worktree api");
        });
        assert_eq!(out, "created worktree api branch=\"feat/x\"\n");
    }

    #[test]
    fn warnings_and_errors_are_labelled() {
        let out = capture(Level::INFO, || {
            tracing::warn!("fetch failed");
            tracing::error!("clone failed");
        });
        assert_eq!(out, "warning: fetch failed\nerror: clone failed\n");
    }

    #[test]
    fn no_timestamp_or_module_path() {
        let out = capture(Level::INFO, || tracing::info!("hello"));
        assert!(!out.contains("tix_cli"), "{out}");
        assert!(!out.contains("INFO"), "{out}");
        assert_eq!(out, "hello\n");
    }

    #[test]
    fn quiet_drops_info() {
        let out = capture(Level::WARN, || {
            tracing::info!("removed api");
            tracing::warn!("left dangling");
        });
        assert_eq!(out, "warning: left dangling\n");
    }

    #[test]
    fn only_debug_and_trace_are_detailed() {
        assert!(!detailed(Level::ERROR));
        assert!(!detailed(Level::WARN));
        assert!(!detailed(Level::INFO));
        assert!(detailed(Level::DEBUG));
        assert!(detailed(Level::TRACE));
    }
}
