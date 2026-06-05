//! Executes a parsed [`Config`](crate::config::Config) against the filesystem.
//!
//! Each directive's executor runs in declaration order. Executors are serial
//! on purpose: link creation can depend on shell output from an earlier step.

mod action;
mod backup;
mod clean;
mod core;
mod create;
mod link;
mod path;
mod shell;
mod sink;

pub use action::{Action, ActionKind};
pub use core::{ApplyError, Engine, Tally};
pub use sink::{IoStreamsSink, NullSink, OutputSink};
