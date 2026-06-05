//! The engine core: dispatch loop, defaults merging, and shared helpers.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::{CleanOptions, Config, Defaults, DirectiveKind, LinkOptions, ShellOptions};

use super::action::{Action, ActionKind};
use super::backup::BackupWriter;
use super::path::{SystemPathEnv, expand};
use super::sink::{NullSink, OutputSink};

/// Counts what happened across one [`Engine::apply`] call. Updated directly by
/// directive executors.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    /// Links already correct (no-op).
    pub links_ok: u32,
    /// Links newly created.
    pub links_created: u32,
    /// Stale links replaced.
    pub links_relinked: u32,
    /// Non-symlink targets backed up.
    pub links_backed_up: u32,
    /// Shell commands run.
    pub shell_run: u32,
    /// Shell commands that exited non-zero.
    pub shell_failed: u32,
    /// Dead symlinks removed.
    pub cleaned: u32,
    /// Directories created.
    pub created: u32,
}

/// A fatal error that aborts an `apply` run. Per-entry failures (one bad link,
/// one failing shell command) do not abort — they are tallied and logged.
#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    /// The run was cancelled (e.g. SIGINT) between directives.
    #[error("apply cancelled")]
    Cancelled,
    /// A directive of an unknown kind was encountered (defensive; the parser
    /// rejects these earlier).
    #[error("engine: unknown directive {kind:?} at line {line}")]
    UnknownDirective {
        /// The offending directive key.
        kind: String,
        /// 1-based source line.
        line: usize,
    },
}

/// The running defaults as the engine iterates. Later `defaults:` directives
/// override earlier values key-by-key.
#[derive(Debug, Default, Clone)]
pub(crate) struct MergedDefaults {
    pub link: LinkOptions,
    pub shell: ShellOptions,
    pub clean: CleanOptions,
}

impl MergedDefaults {
    fn merge(&mut self, d: &Defaults) {
        if let Some(link) = &d.link {
            self.link = merge_link_opts(&self.link, link);
        }
        if let Some(shell) = &d.shell {
            self.shell = merge_shell_opts(&self.shell, shell);
        }
        if let Some(clean) = &d.clean {
            self.clean = merge_clean_opts(&self.clean, clean);
        }
    }
}

/// Applies a parsed [`Config`] to the filesystem.
///
/// `base_dir` is the directory containing the profile (usually the dotfiles
/// repo root); all relative link sources resolve against it. Set `dry_run` to
/// record intended actions without touching the filesystem — executors still
/// inspect the FS to decide what *would* happen but skip every mutation.
pub struct Engine<S: OutputSink = NullSink> {
    /// Absolute base directory.
    pub base_dir: PathBuf,
    /// Whether to plan without mutating the filesystem.
    pub dry_run: bool,
    /// Whether to skip `shell:` directives entirely (links/dirs still run).
    pub skip_shell: bool,
    /// Progress sink.
    pub(crate) sink: S,
    /// Recorded actions (populated in both real and dry-run modes).
    pub actions: Vec<Action>,
    /// Accumulated defaults across `defaults:` directives.
    pub(crate) defaults: MergedDefaults,
    /// Lazily created session backup directory.
    pub(crate) backup: BackupWriter,
    /// Shared timestamp tag for all backups in this run.
    pub(crate) backup_tag: String,
    /// Backup root's home directory. `None` falls back to `$HOME`.
    pub(crate) backup_home: Option<PathBuf>,
    /// Cancellation flag, checked between directives.
    pub(crate) cancel: Arc<AtomicBool>,
}

impl Engine<NullSink> {
    /// Build an engine for `base_dir` with no output sink. `base_dir` should be
    /// absolute.
    #[must_use]
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Engine {
            base_dir: base_dir.into(),
            dry_run: false,
            skip_shell: false,
            sink: NullSink,
            actions: Vec::new(),
            defaults: MergedDefaults::default(),
            backup: BackupWriter::default(),
            backup_tag: String::new(),
            backup_home: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl<S: OutputSink> Engine<S> {
    /// Replace the output sink (builder style).
    pub fn with_sink<T: OutputSink>(self, sink: T) -> Engine<T> {
        Engine {
            base_dir: self.base_dir,
            dry_run: self.dry_run,
            skip_shell: self.skip_shell,
            sink,
            actions: self.actions,
            defaults: self.defaults,
            backup: self.backup,
            backup_tag: self.backup_tag,
            backup_home: self.backup_home,
            cancel: self.cancel,
        }
    }

    /// Set dry-run mode (builder style).
    #[must_use]
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Skip `shell:` directives entirely (builder style). Links, cleans, and
    /// directory creation still run; shell commands are neither executed nor
    /// recorded.
    #[must_use]
    pub fn skip_shell(mut self, skip_shell: bool) -> Self {
        self.skip_shell = skip_shell;
        self
    }

    /// Install a cancellation flag, checked between directives.
    #[must_use]
    pub fn with_cancel(mut self, cancel: Arc<AtomicBool>) -> Self {
        self.cancel = cancel;
        self
    }

    /// Record an action.
    pub(crate) fn record(
        &mut self,
        kind: ActionKind,
        from: impl Into<String>,
        to: impl Into<String>,
    ) {
        self.actions.push(Action {
            kind,
            from: from.into(),
            to: to.into(),
            dry_run: self.dry_run,
        });
    }

    /// Run every directive in `cfg` against the filesystem, returning a
    /// [`Tally`]. Non-fatal issues accumulate in the tally without aborting.
    pub fn apply(&mut self, cfg: &Config) -> Result<Tally, ApplyError> {
        let mut tally = Tally::default();

        for d in &cfg.directives {
            if self.cancel.load(Ordering::Relaxed) {
                return Err(ApplyError::Cancelled);
            }

            log::debug!("directive kind={} line={}", d.kind.name(), d.line);

            match &d.kind {
                DirectiveKind::Defaults(defaults) => self.defaults.merge(defaults),
                DirectiveKind::Link(link) => self.run_link(link, &mut tally),
                DirectiveKind::Shell(_) if self.skip_shell => {
                    log::debug!("skipping shell directive (--no-shell)");
                }
                DirectiveKind::Shell(shell) => self.run_shell(shell, &mut tally),
                DirectiveKind::Clean(clean) => self.run_clean(clean, &mut tally),
                DirectiveKind::Create(create) => self.run_create(create, &mut tally),
            }
        }

        Ok(tally)
    }

    /// Resolve a relative source against the base directory; absolute sources
    /// are returned verbatim.
    pub(crate) fn resolve_base(&self, source: &str) -> PathBuf {
        super::path::resolve_base(&self.base_dir, source)
    }

    /// Expand `~` and `$VAR` in a path using the process environment.
    pub(crate) fn expand_path(path: &str) -> String {
        expand(path, &SystemPathEnv)
    }

    /// Whether the session backup directory has been initialized.
    pub(crate) fn backup_initialized(&self) -> bool {
        self.backup.is_initialized()
    }

    /// Set the timestamp tag used for the session backup directory.
    pub fn set_backup_tag(&mut self, tag: impl Into<String>) {
        self.backup_tag = tag.into();
    }

    /// Override the home directory under which backups are written. When unset,
    /// `$HOME` is used.
    #[must_use]
    pub fn with_backup_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.backup_home = Some(home.into());
        self
    }

    /// The backup home: the explicit override, else `$HOME`.
    pub(crate) fn backup_home_dir(&self) -> Option<PathBuf> {
        self.backup_home.clone().or_else(home_dir)
    }
}

// ── option merging (overlay non-None fields onto base) ──────────────────────

/// Return a copy of `base` with every set field of `overlay` overriding it.
/// Slice fields replace wholesale when the overlay is non-empty.
pub(crate) fn merge_link_opts(base: &LinkOptions, overlay: &LinkOptions) -> LinkOptions {
    let mut out = base.clone();
    if overlay.path.is_some() {
        out.path.clone_from(&overlay.path);
    }
    if overlay.create.is_some() {
        out.create = overlay.create;
    }
    if overlay.relink.is_some() {
        out.relink = overlay.relink;
    }
    if overlay.force.is_some() {
        out.force = overlay.force;
    }
    if overlay.relative.is_some() {
        out.relative = overlay.relative;
    }
    if overlay.glob.is_some() {
        out.glob = overlay.glob;
    }
    if overlay.ignore_missing.is_some() {
        out.ignore_missing = overlay.ignore_missing;
    }
    if overlay.backup.is_some() {
        out.backup = overlay.backup;
    }
    if overlay.link_type.is_some() {
        out.link_type.clone_from(&overlay.link_type);
    }
    if overlay.canonicalize.is_some() {
        out.canonicalize = overlay.canonicalize;
    }
    if overlay.prefix.is_some() {
        out.prefix.clone_from(&overlay.prefix);
    }
    if !overlay.exclude.is_empty() {
        out.exclude.clone_from(&overlay.exclude);
    }
    out
}

pub(crate) fn merge_shell_opts(base: &ShellOptions, overlay: &ShellOptions) -> ShellOptions {
    let mut out = base.clone();
    if overlay.stdin.is_some() {
        out.stdin = overlay.stdin;
    }
    if overlay.stdout.is_some() {
        out.stdout = overlay.stdout;
    }
    if overlay.stderr.is_some() {
        out.stderr = overlay.stderr;
    }
    if overlay.quiet.is_some() {
        out.quiet = overlay.quiet;
    }
    out
}

pub(crate) fn merge_clean_opts(base: &CleanOptions, overlay: &CleanOptions) -> CleanOptions {
    let mut out = base.clone();
    if overlay.force.is_some() {
        out.force = overlay.force;
    }
    if overlay.recursive.is_some() {
        out.recursive = overlay.recursive;
    }
    out
}

/// `opt.unwrap_or(fallback)` for the tri-state option fields.
pub(crate) fn bool_or(opt: Option<bool>, fallback: bool) -> bool {
    opt.unwrap_or(fallback)
}

/// The user's home directory, used for backup-root initialization.
pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

/// Whether `path` is the base dir or resolves into it (for clean's safety
/// check). Accepts both the literal and symlink-resolved forms of the base.
pub(crate) fn base_candidates(base_dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![super::path::clean(&base_dir.to_string_lossy())];
    if let Ok(resolved) = std::fs::canonicalize(base_dir) {
        let resolved = super::path::clean(&resolved.to_string_lossy());
        if !out.contains(&resolved) {
            out.push(resolved);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_overlays_only_set_fields() {
        let base = LinkOptions {
            create: Some(true),
            relink: Some(false),
            ..LinkOptions::default()
        };
        let overlay = LinkOptions {
            relink: Some(true), // overrides
            force: Some(true),  // adds
            ..LinkOptions::default()
        };
        let merged = merge_link_opts(&base, &overlay);
        assert_eq!(merged.create, Some(true), "untouched field preserved");
        assert_eq!(merged.relink, Some(true), "set field overridden");
        assert_eq!(merged.force, Some(true), "new field added");
    }

    #[test]
    fn merge_none_does_not_clear() {
        // A None overlay field must NOT clear an explicit base value — this is
        // the unset-vs-explicit distinction.
        let base = LinkOptions {
            create: Some(false),
            ..LinkOptions::default()
        };
        let merged = merge_link_opts(&base, &LinkOptions::default());
        assert_eq!(merged.create, Some(false));
    }

    #[test]
    fn bool_or_uses_fallback_when_none() {
        assert!(bool_or(None, true));
        assert!(!bool_or(Some(false), true));
        assert!(bool_or(Some(true), false));
    }

    #[test]
    fn exclude_replaces_only_when_nonempty() {
        let base = LinkOptions {
            exclude: vec!["a".into()],
            ..LinkOptions::default()
        };
        let merged = merge_link_opts(&base, &LinkOptions::default());
        assert_eq!(
            merged.exclude,
            vec!["a".to_string()],
            "empty overlay keeps base"
        );
        let overlay = LinkOptions {
            exclude: vec!["b".into()],
            ..LinkOptions::default()
        };
        let merged = merge_link_opts(&base, &overlay);
        assert_eq!(
            merged.exclude,
            vec!["b".to_string()],
            "nonempty overlay replaces"
        );
    }
}
