//! Recorded engine actions.
//!
//! `dfm diff` groups and colorizes output by [`ActionKind`], and tests assert
//! on recorded actions without scraping log text. Both real and dry-run
//! executions record the same structures, so diff and apply share planning.

use std::fmt;

/// Classifies a recorded engine action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    /// A new symlink was created.
    LinkCreate,
    /// A stale symlink was replaced.
    LinkRelink,
    /// An idempotent no-op; recorded for completeness.
    LinkExists,
    /// An existing non-symlink was moved aside.
    LinkBackup,
    /// A link that would run but was blocked by a conflict (no relink/force).
    LinkSkip,
    /// A shell command was run.
    ShellRun,
    /// A dead symlink was removed.
    CleanRemove,
    /// A directory was created.
    CreateDir,
    /// An idempotent directory no-op (already present).
    CreateExists,
}

impl ActionKind {
    /// The short label used in diagnostic and diff output.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ActionKind::LinkCreate => "link",
            ActionKind::LinkRelink => "relink",
            ActionKind::LinkExists => "link-ok",
            ActionKind::LinkBackup => "backup",
            ActionKind::LinkSkip => "skip",
            ActionKind::ShellRun => "shell",
            ActionKind::CleanRemove => "clean",
            ActionKind::CreateDir => "mkdir",
            ActionKind::CreateExists => "mkdir-ok",
        }
    }
}

impl fmt::Display for ActionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One recorded step produced by the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// What kind of action this is.
    pub kind: ActionKind,
    /// The primary subject: link target, directory path, or command string.
    pub from: String,
    /// The secondary target when relevant: symlink destination, backup
    /// destination path, or shell command description.
    pub to: String,
    /// True when this action was recorded but not executed (dry-run).
    pub dry_run: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_stable() {
        assert_eq!(ActionKind::LinkCreate.label(), "link");
        assert_eq!(ActionKind::LinkExists.label(), "link-ok");
        assert_eq!(ActionKind::CreateExists.label(), "mkdir-ok");
        assert_eq!(ActionKind::CleanRemove.to_string(), "clean");
    }
}
