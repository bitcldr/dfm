//! Shared profile-path resolution for `apply` and `diff`.

use std::path::{Path, PathBuf};

/// A command-layer error. Boxed so each command can return a uniform type.
pub type CliError = Box<dyn std::error::Error>;

/// A command result.
pub type CliResult = Result<(), CliError>;

/// Resolve which profile files to operate on.
///
/// An explicit `config_path` wins unconditionally (positionals are ignored).
/// Otherwise each profile name maps to `<base>/profiles/<name>.conf.yaml` and
/// must exist.
pub fn resolve_profile_paths(
    base_abs: &Path,
    config_path: Option<&Path>,
    profiles: &[String],
) -> Result<Vec<PathBuf>, CliError> {
    if let Some(cfg) = config_path {
        return Ok(vec![std::path::absolute(cfg)?]);
    }

    if profiles.is_empty() {
        return Err("at least one profile is required".into());
    }

    let mut out = Vec::with_capacity(profiles.len());
    for name in profiles {
        let p = base_abs.join("profiles").join(format!("{name}.conf.yaml"));
        if !p.exists() {
            return Err(format!("profile {name:?} not found at {}", p.display()).into());
        }
        out.push(p);
    }
    Ok(out)
}
