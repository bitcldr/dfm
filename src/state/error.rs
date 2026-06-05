//! State load/save errors, each wrapped with a `state:` prefix.

use std::io;
use std::path::PathBuf;

/// A failure reading or writing the state file.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// The home directory could not be determined ($HOME unset).
    #[error("state: home dir: $HOME is not set")]
    NoHome,
    /// Reading the state file failed.
    #[error("state: read {path}: {source}")]
    Read {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        source: io::Error,
    },
    /// Parsing the state JSON failed.
    #[error("state: parse {path}: {source}")]
    Parse {
        /// Path whose contents failed to parse.
        path: PathBuf,
        /// Underlying JSON error.
        source: serde_json::Error,
    },
    /// Creating the parent directory failed.
    #[error("state: mkdir: {0}")]
    Mkdir(io::Error),
    /// Serializing the state failed.
    #[error("state: marshal: {0}")]
    Marshal(serde_json::Error),
    /// Writing the state file failed.
    #[error("state: write {path}: {source}")]
    Write {
        /// Path that failed to write.
        path: PathBuf,
        /// Underlying I/O error.
        source: io::Error,
    },
}
