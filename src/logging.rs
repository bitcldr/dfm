//! A minimal `log` backend that writes diagnostics to stderr.
//!
//! Progress and data output go through [`IoStreams`](crate::iostreams); this
//! logger handles the `log::warn!` / `log::debug!` diagnostics emitted by the
//! engine. The level is chosen from the CLI flags:
//!
//! - `--verbose` → `Debug` (full tracing)
//! - `--quiet`   → `Warn` (warnings and errors still surface)
//! - otherwise   → `Info`

use std::io::Write;

use log::{Level, LevelFilter, Log, Metadata, Record};

/// Writes `[LEVEL] message` lines to stderr for records at or above the
/// configured level. Debug records also include the module path.
struct StderrLogger {
    level: LevelFilter,
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let mut stderr = std::io::stderr().lock();
        let _ = match record.level() {
            Level::Debug | Level::Trace => writeln!(
                stderr,
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            ),
            level => writeln!(stderr, "[{level}] {}", record.args()),
        };
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

/// Install the stderr logger at `level`. Safe to call once per process; a
/// second call is ignored (so tests that each build a CLI don't panic).
pub fn init(level: LevelFilter) {
    let logger = Box::new(StderrLogger { level });
    // set_boxed_logger fails if a logger is already installed — ignore that,
    // it just means another caller (or a prior test) won already.
    if log::set_boxed_logger(logger).is_ok() {
        log::set_max_level(level);
    }
}

/// Map the verbose/quiet flags to a level filter.
///
/// `verbose` wins if both are somehow set (the CLI rejects that combination
/// earlier, so this is only a defensive default).
#[must_use]
pub fn level_from_flags(verbose: bool, quiet: bool) -> LevelFilter {
    if verbose {
        LevelFilter::Debug
    } else if quiet {
        LevelFilter::Warn
    } else {
        LevelFilter::Info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_mapping() {
        assert_eq!(level_from_flags(true, false), LevelFilter::Debug);
        assert_eq!(level_from_flags(false, true), LevelFilter::Warn);
        assert_eq!(level_from_flags(false, false), LevelFilter::Info);
        // verbose wins defensively
        assert_eq!(level_from_flags(true, true), LevelFilter::Debug);
    }
}
