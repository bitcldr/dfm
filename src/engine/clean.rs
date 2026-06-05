//! The `clean:` directive executor: remove dead symlinks.
//!
//! A dead (dangling) symlink is removed only when it points back into the base
//! directory, so unrelated links are never touched — unless `force` is set.
//! Live symlinks and symlinks pointing outside the base dir are left alone.

use std::path::{Path, PathBuf};

use crate::config::Clean;

use super::action::ActionKind;
use super::core::{Engine, Tally, base_candidates, bool_or, merge_clean_opts};
use super::sink::OutputSink;

impl<S: OutputSink> Engine<S> {
    /// Remove dead symlinks in the target directories.
    pub(crate) fn run_clean(&mut self, c: &Clean, tally: &mut Tally) {
        // Collect every prefix that counts as "inside the base dir". On some
        // platforms a temp dir like /var/folders/... canonicalizes to
        // /private/var/folders/..., so both forms must be accepted.
        let candidates = base_candidates(&self.base_dir);

        for entry in &c.entries {
            let opts = merge_clean_opts(&self.defaults.clean, &entry.options);
            let target = Self::expand_path(&entry.target);
            let force = bool_or(opts.force, false);
            let recursive = bool_or(opts.recursive, false);
            self.clean_dir(Path::new(&target), &candidates, force, recursive, tally);
        }
    }

    fn clean_dir(
        &mut self,
        dir: &Path,
        candidates: &[PathBuf],
        force: bool,
        recursive: bool,
        tally: &mut Tally,
    ) {
        match std::fs::metadata(dir) {
            Ok(md) if !md.is_dir() => return,
            Err(_) => return, // missing or unreadable → nothing to clean
            Ok(_) => {}
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("clean {}: {e}", dir.display());
                return;
            }
        };

        for de in entries.flatten() {
            let path = de.path();
            let Ok(info) = std::fs::symlink_metadata(&path) else {
                continue;
            };

            if recursive && info.is_dir() {
                self.clean_dir(&path, candidates, force, recursive, tally);
                continue;
            }

            if !info.file_type().is_symlink() {
                continue;
            }

            // Only broken symlinks are candidates: if it still resolves, skip.
            if std::fs::metadata(&path).is_ok() {
                log::debug!("clean skip {}: still resolves", path.display());
                continue;
            }

            let Ok(points) = std::fs::read_link(&path) else {
                continue;
            };
            let points_abs = if points.is_absolute() {
                points.clone()
            } else {
                path.parent().unwrap_or(Path::new("")).join(&points)
            };

            if !force && !super::path::is_inside_any(&points_abs, candidates) {
                log::debug!(
                    "clean skip {}: points outside base dir (points={})",
                    path.display(),
                    points_abs.display()
                );
                continue;
            }
            log::debug!(
                "clean remove {} -> {} force={force}",
                path.display(),
                points_abs.display()
            );

            if !self.dry_run
                && let Err(e) = std::fs::remove_file(&path)
            {
                log::warn!("remove {}: {e}", path.display());
                continue;
            }

            let from = path.to_string_lossy().into_owned();
            let to = points_abs.to_string_lossy().into_owned();
            self.sink.removed_dead_link(&from, &to);
            self.record(ActionKind::CleanRemove, from, to);
            tally.cleaned += 1;
        }
    }
}
