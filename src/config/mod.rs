//! Profile YAML parsing into a typed, order-preserving [`Config`].
//!
//! The profile format is documented in `docs/yaml-spec.md`. Unknown directives
//! and options are rejected with a 1-based `line:col` reference.

mod error;
mod model;
mod parse;

pub use error::{LoadError, ParseError};
pub use model::{
    Clean, CleanEntry, CleanOptions, Config, Create, CreateEntry, Defaults, Directive,
    DirectiveKind, Link, LinkEntry, LinkOptions, Shell, ShellEntry, ShellOptions,
};
pub use parse::{load, parse_str};
