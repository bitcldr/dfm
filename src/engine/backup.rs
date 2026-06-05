//! Session backup directory management.
//!
//! When a link target is a pre-existing non-symlink, it is moved aside into a
//! per-session backup directory (`~/.dotfiles-backup/<timestamp>/...`) before
//! the symlink is created. Backups are reversible, which keeps `apply`
//! idempotent. One [`BackupWriter`] is used per engine run, sharing a single
//! timestamp tag across all backups.

use std::path::{Path, PathBuf};

use super::path::strip_root;

/// Lazily owns the session backup directory. Created on the first conflict.
#[derive(Debug, Default)]
pub(crate) struct BackupWriter {
    /// Absolute path of the session backup dir, empty until initialized.
    root: PathBuf,
}

impl BackupWriter {
    /// Whether the backup root has been initialized yet.
    pub(crate) fn is_initialized(&self) -> bool {
        !self.root.as_os_str().is_empty()
    }
}

/// Outcome of backing up a path: where it was (or would be) moved.
pub(crate) struct BackupPlan {
    /// The destination path under the backup root.
    pub dst: PathBuf,
}

/// Initialize the backup root under `home`, using `tag` as the shared
/// timestamp segment. In dry-run mode the directory is not created.
pub(crate) fn init_root(
    writer: &mut BackupWriter,
    home: &Path,
    tag: &str,
    dry_run: bool,
) -> std::io::Result<()> {
    writer.root = home.join(".dotfiles-backup").join(tag);
    if dry_run {
        return Ok(());
    }
    std::fs::create_dir_all(&writer.root)
}

/// Move `src` aside into the backup root, mirroring its full absolute path so
/// two backups of the same basename do not collide. The leading `/` is dropped
/// before joining so the absolute `src` does not reset the join back to root.
///
/// Returns the destination. In dry-run mode no filesystem change is made.
pub(crate) fn back_up(
    writer: &BackupWriter,
    src: &Path,
    dry_run: bool,
) -> std::io::Result<BackupPlan> {
    debug_assert!(
        writer.is_initialized(),
        "backup root must be initialized first"
    );

    let dst = writer.root.join(strip_root(src));

    if !dry_run {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(src, &dst)?;
    }

    Ok(BackupPlan { dst })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_mirrors_absolute_path_under_root() {
        let home = tempfile::tempdir().unwrap();
        let mut w = BackupWriter::default();
        init_root(&mut w, home.path(), "20260604T000000Z", false).unwrap();

        // create a file to back up
        let src_dir = tempfile::tempdir().unwrap();
        let src = src_dir.path().join("conf");
        std::fs::write(&src, b"hi").unwrap();

        let plan = back_up(&w, &src, false).unwrap();

        // dst must be under the backup root, mirroring src's path (sans leading /)
        assert!(plan.dst.starts_with(home.path().join(".dotfiles-backup")));
        assert!(plan.dst.ends_with(strip_root(&src)));
        assert!(plan.dst.exists(), "file should have been moved");
        assert!(!src.exists(), "source should be gone after rename");
    }

    #[test]
    fn dry_run_records_destination_without_moving() {
        let home = tempfile::tempdir().unwrap();
        let mut w = BackupWriter::default();
        init_root(&mut w, home.path(), "tag", true).unwrap();

        let src_dir = tempfile::tempdir().unwrap();
        let src = src_dir.path().join("conf");
        std::fs::write(&src, b"hi").unwrap();

        let plan = back_up(&w, &src, true).unwrap();
        assert!(src.exists(), "dry-run must not move the file");
        assert!(!plan.dst.exists(), "dry-run must not create the backup");
    }
}
