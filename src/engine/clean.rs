//! The `clean:` directive executor: remove dead symlinks.
//!
//! A dead (dangling) symlink is removed only when it points back into the base
//! directory, so unrelated links are never touched. Live symlinks and symlinks
//! pointing outside the base dir are left alone.
//!
//! `force: true` widens this to remove dead links regardless of where they
//! point — but only within bounds: the scanned directory must resolve under
//! `$HOME`, and recursion is capped (see [`MAX_FORCE_DEPTH`]). This keeps the
//! useful "clean dead links anywhere under this dir" behavior while preventing
//! a `clean: { "/": { force: true, recursive: true } }` from walking the whole
//! filesystem.

use std::path::{Path, PathBuf};

use crate::config::Clean;

use super::action::ActionKind;
use super::core::{Engine, Tally, base_candidates, bool_or, merge_clean_opts};
use super::sink::OutputSink;

/// Maximum directory depth `clean` descends into when `force` + `recursive`
/// are both set. Non-force cleans are unaffected. Bounds the blast radius of a
/// forced recursive clean rooted high in the tree.
pub(crate) const MAX_FORCE_DEPTH: usize = 5;

impl<S: OutputSink> Engine<S> {
    /// Remove dead symlinks in the target directories.
    pub(crate) fn run_clean(&mut self, c: &Clean, tally: &mut Tally) {
        // Collect every prefix that counts as "inside the base dir". On some
        // platforms a temp dir like /var/folders/... canonicalizes to
        // /private/var/folders/..., so both forms must be accepted.
        let candidates = base_candidates(&self.base_dir);
        let home = self.home_dir();

        for entry in &c.entries {
            let opts = merge_clean_opts(&self.defaults.clean, &entry.options);
            let target = Self::expand_path(&entry.target);
            let force = bool_or(opts.force, false);
            let recursive = bool_or(opts.recursive, false);

            // `force` drops the inside-base guard, so require the scanned tree
            // to sit under $HOME — refuse to forcibly clean system locations.
            if force && !force_target_allowed(&target, home.as_deref()) {
                log::warn!("clean: force ignored for {target} (outside $HOME)");
                continue;
            }

            self.clean_dir(Path::new(&target), &candidates, force, recursive, 0, tally);
        }
    }

    fn clean_dir(
        &mut self,
        dir: &Path,
        candidates: &[PathBuf],
        force: bool,
        recursive: bool,
        depth: usize,
        tally: &mut Tally,
    ) {
        // Cap recursion depth under `force` to bound a forced recursive clean.
        if force && depth > MAX_FORCE_DEPTH {
            log::debug!("clean: max force depth reached at {}", dir.display());
            return;
        }

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
                self.clean_dir(&path, candidates, force, recursive, depth + 1, tally);
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
            // Resolve a relative target against the symlink's *canonical*
            // parent. The link itself dangles, but its parent exists, so
            // canonicalize succeeds and collapses any symlinked intermediate
            // directories — the containment check then compares real paths,
            // not just textually-normalized ones.
            let points_abs = if points.is_absolute() {
                super::path::clean(&points.to_string_lossy())
            } else {
                let parent = path.parent().unwrap_or(Path::new(""));
                let parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
                super::path::clean(&parent.join(&points).to_string_lossy())
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

/// Whether a `force` clean is allowed for `target`: the directory must resolve
/// under `home`. With no home known, force is refused (fail safe).
///
/// Both sides are canonicalized when they exist (falling back to lexical
/// cleaning otherwise) so a symlinked `$HOME` or scan target is compared on
/// real paths — symmetric with the containment check used during the scan.
fn force_target_allowed(target: &str, home: Option<&Path>) -> bool {
    let Some(home) = home else {
        return false;
    };
    let target_abs = canonical_or_clean(Path::new(target));
    let home_abs = canonical_or_clean(home);
    super::path::is_inside_any(&target_abs, std::slice::from_ref(&home_abs))
        || target_abs == home_abs
}

/// Canonicalize `p` if it exists on disk; otherwise fall back to making it
/// absolute and lexically cleaning it.
fn canonical_or_clean(p: &Path) -> PathBuf {
    if let Ok(real) = std::fs::canonicalize(p) {
        return super::path::clean(&real.to_string_lossy());
    }
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    super::path::clean(&abs.to_string_lossy())
}
