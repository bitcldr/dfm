//! The `create:` directive executor: idempotent `mkdir -p` with optional mode.

use std::os::unix::fs::PermissionsExt;

use crate::config::Create;

use super::action::ActionKind;
use super::core::{Engine, Tally};
use super::sink::OutputSink;

impl<S: OutputSink> Engine<S> {
    /// Create the listed directories. Idempotent: existing paths are left alone
    /// (no mode update). The default mode is `0o777` (umask still applies).
    pub(crate) fn run_create(&mut self, c: &Create, tally: &mut Tally) {
        for entry in &c.entries {
            let path = Self::expand_path(&entry.path);
            let mode = entry.mode.unwrap_or(0o777);

            match std::fs::symlink_metadata(&path) {
                Ok(_) => {
                    log::debug!("create stat path={path} exists=true");
                    self.sink.path_exists(&path);
                    self.record(ActionKind::CreateExists, path, "");
                    continue;
                }
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    log::warn!("stat {path}: {e}");
                    continue;
                }
                Err(_) => {} // not found → create it
            }

            if !self.dry_run {
                if let Err(e) = std::fs::create_dir_all(&path) {
                    log::warn!("create {path}: {e}");
                    continue;
                }
                if let Err(e) =
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                {
                    log::debug!("chmod failed path={path} err={e}");
                } else {
                    log::debug!("chmod path={path} mode={mode:04o}");
                }
            }

            self.sink.created(&path);
            self.record(ActionKind::CreateDir, path, "");
            tally.created += 1;
        }
    }
}
