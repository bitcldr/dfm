//! Config parse/load errors.
//!
//! `ParseError` records a 1-based line/column when known and renders as
//! `line L:C: msg` (or just `msg` when the position is unknown).

use std::fmt;
use std::io;

/// A profile parse failure, with a 1-based line/column when available.
///
/// Cloneable and comparable so callers (and tests) can assert on the exact
/// position and message without scraping a formatted string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line; 0 means "unknown".
    pub line: usize,
    /// 1-based column; 0 means "unknown".
    pub column: usize,
    /// Human-readable message (no position prefix).
    pub msg: String,
}

impl ParseError {
    /// Build a `ParseError` at a known 1-based position.
    pub fn at(line: usize, column: usize, msg: impl Into<String>) -> Self {
        Self {
            line,
            column,
            msg: msg.into(),
        }
    }

    /// Build a `ParseError` with no position information.
    pub fn msg(msg: impl Into<String>) -> Self {
        Self {
            line: 0,
            column: 0,
            msg: msg.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "line {}:{}: {}", self.line, self.column, self.msg)
        } else {
            f.write_str(&self.msg)
        }
    }
}

impl std::error::Error for ParseError {}

/// Failure to load a profile from disk: either the file could not be opened or
/// its contents did not parse. Renders as `config: open …` or
/// `config: parse …` depending on which stage failed.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The profile file could not be opened or read.
    #[error("config: open {path}: {source}")]
    Open {
        /// The path that failed to open.
        path: String,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// The profile file opened but its contents did not parse.
    #[error("config: parse {path}: {source}")]
    Parse {
        /// The path whose contents failed to parse.
        path: String,
        /// The underlying parse error.
        source: ParseError,
    },
}
