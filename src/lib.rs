//! dfm — a standalone single-binary dotfiles manager.
//!
//! This library crate holds all of dfm's logic; the `dfm` binary is a thin
//! wrapper around it. The profile YAML format is defined in
//! `docs/yaml-spec.md`.

pub mod cli;
pub mod config;
pub mod engine;
pub mod iostreams;
pub mod state;
pub mod version;
