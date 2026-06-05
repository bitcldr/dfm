//! Per-machine application state, persisted as JSON.
//!
//! The file lives at `~/.local/state/dfm/state.json` (XDG default) and records
//! which profiles were last applied, when, and which symlinks the engine
//! created. `dfm status` reads it; `dfm doctor` uses it to verify drift.
//!
//! The on-disk JSON shape is:
//! `{ "last_applied": [...], "applied_at": "<rfc3339>", "links": [...] }`.

mod error;
mod schema;
mod store;

pub use error::StateError;
pub use schema::{Link, State};
pub use store::{Store, load, path, save};
