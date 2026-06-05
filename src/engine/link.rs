//! The `link:` directive executor: create, relink, skip, or back-up-and-replace.

use std::path::{Path, PathBuf};

use crate::config::{Link, LinkEntry};

use super::action::ActionKind;
use super::backup;
use super::core::{Engine, Tally, bool_or, merge_link_opts};
use super::path::{default_source, has_glob_chars, rel};
use super::sink::OutputSink;

/// What currently exists at a link target path.
enum Existing {
    /// Nothing exists there.
    Absent,
    /// A symlink, with its current destination.
    Symlink(String),
    /// A regular file or directory.
    NotSymlink,
}

impl<S: OutputSink> Engine<S> {
    /// Execute a `link:` directive. A single failing entry is logged as a
    /// warning and does not abort the directive.
    pub(crate) fn run_link(&mut self, l: &Link, tally: &mut Tally) {
        for entry in &l.entries {
            if let Err(e) = self.link_one(entry, tally) {
                log::warn!("link {}: {e}", entry.target);
            }
        }
    }

    fn link_one(&mut self, entry: &LinkEntry, tally: &mut Tally) -> Result<(), String> {
        let opts = merge_link_opts(&self.defaults.link, &entry.options);

        // Source comes from opts.path; if absent, the target's basename with a
        // leading dot stripped (e.g. "~/.vim" → "vim").
        let source = opts
            .path
            .clone()
            .unwrap_or_else(|| default_source(&entry.target));
        let source_abs = self.resolve_base(&source);

        let link_path = PathBuf::from(Self::expand_path(&entry.target));
        let link_path = std::path::absolute(&link_path).map_err(|e| format!("abs: {e}"))?;
        log::debug!(
            "link target={} source={}",
            link_path.display(),
            source_abs.display()
        );

        if bool_or(opts.glob, false) && has_glob_chars(&source) {
            return Err("glob: not yet supported".to_string());
        }

        // Validate the source exists unless ignore-missing is set.
        if !bool_or(opts.ignore_missing, false) {
            match std::fs::symlink_metadata(&source_abs) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err(format!("nonexistent source {}", source_abs.display()));
                }
                Err(e) => return Err(format!("stat source: {e}")),
            }
        }

        // Optionally create the link's parent directory.
        if bool_or(opts.create, false)
            && !self.dry_run
            && let Some(parent) = link_path.parent()
        {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir parent: {e}"))?;
        }

        // The desired link text: a relative path when relative=true, else the
        // absolute source.
        let desired = if bool_or(opts.relative, false) {
            let dir = link_path.parent().unwrap_or(Path::new("/"));
            rel(dir, &source_abs)
                .map(|p| p.to_string_lossy().into_owned())
                .ok_or_else(|| "relative: no path".to_string())?
        } else {
            source_abs.to_string_lossy().into_owned()
        };

        let link_type = opts.link_type.as_deref().unwrap_or("symlink");
        if link_type != "symlink" {
            return Err(format!("link type {link_type:?} not supported"));
        }

        match Self::inspect(&link_path)? {
            Existing::Absent => self.create_symlink(&link_path, &desired, tally),
            Existing::Symlink(current) => {
                log::debug!(
                    "existing symlink={} current={current} desired={desired}",
                    link_path.display()
                );
                if current == desired {
                    self.sink.link_ok(&entry.target, &desired);
                    self.record(
                        ActionKind::LinkExists,
                        link_path.to_string_lossy(),
                        &desired,
                    );
                    tally.links_ok += 1;
                    return Ok(());
                }
                if bool_or(opts.relink, false) || bool_or(opts.force, false) {
                    self.perform_relink(&link_path, &desired)?;
                    self.sink.relinked(&link_path.to_string_lossy(), &desired);
                    self.record(
                        ActionKind::LinkRelink,
                        link_path.to_string_lossy(),
                        &desired,
                    );
                    tally.links_relinked += 1;
                    return Ok(());
                }
                self.record(ActionKind::LinkSkip, link_path.to_string_lossy(), &desired);
                Err(format!(
                    "incorrect link {} -> {current} (want {desired}); enable relink to replace",
                    entry.target
                ))
            }
            Existing::NotSymlink => {
                // Back up the existing file/dir, then replace with the symlink.
                log::debug!("backup decision: backing up {}", link_path.display());
                self.ensure_backup(&link_path)
                    .map_err(|e| format!("backup: {e}"))?;
                tally.links_backed_up += 1;
                self.create_symlink(&link_path, &desired, tally)
            }
        }
    }

    /// Inspect what exists at `link_path` without following the final symlink.
    fn inspect(link_path: &Path) -> Result<Existing, String> {
        match std::fs::symlink_metadata(link_path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Existing::Absent),
            Err(e) => Err(format!("lstat {}: {e}", link_path.display())),
            Ok(md) if md.file_type().is_symlink() => {
                let current = std::fs::read_link(link_path)
                    .map_err(|e| format!("readlink: {e}"))?
                    .to_string_lossy()
                    .into_owned();
                Ok(Existing::Symlink(current))
            }
            Ok(_) => Ok(Existing::NotSymlink),
        }
    }

    fn perform_relink(&self, link_path: &Path, desired: &str) -> Result<(), String> {
        if self.dry_run {
            return Ok(());
        }
        std::fs::remove_file(link_path).map_err(|e| format!("remove stale link: {e}"))?;
        write_symlink(desired, link_path)
    }

    /// Create a symlink (or record it in dry-run) and update the tally.
    fn create_symlink(
        &mut self,
        link_path: &Path,
        target: &str,
        tally: &mut Tally,
    ) -> Result<(), String> {
        if !self.dry_run {
            write_symlink(target, link_path)?;
        }
        self.sink.linked(&link_path.to_string_lossy(), target);
        self.record(ActionKind::LinkCreate, link_path.to_string_lossy(), target);
        tally.links_created += 1;
        Ok(())
    }

    /// Move the existing non-symlink at `link_path` into the session backup dir.
    fn ensure_backup(&mut self, link_path: &Path) -> std::io::Result<()> {
        if self.backup_tag.is_empty() {
            // A fixed-shape UTC tag; the engine sets a real timestamp via the
            // CLI before apply. Default keeps backups grouped per run.
            self.backup_tag = "backup".to_string();
        }
        if !self.backup_initialized() {
            let home = self
                .backup_home_dir()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "home dir"))?;
            let tag = self.backup_tag.clone();
            backup::init_root(&mut self.backup, &home, &tag, self.dry_run)?;
        }
        let plan = backup::back_up(&self.backup, link_path, self.dry_run)?;
        let from = link_path.to_string_lossy().into_owned();
        let to = plan.dst.to_string_lossy().into_owned();
        self.sink.backed_up(&from, &to);
        self.record(ActionKind::LinkBackup, from, to);
        Ok(())
    }
}

/// Raw symlink creation, isolated as the only mutation point.
fn write_symlink(target: &str, link_path: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link_path).map_err(|e| format!("symlink: {e}"))
}
