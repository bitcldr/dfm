//! The typed, order-preserving profile model.
//!
//! A profile is a single `Directive` enum whose variants carry their own
//! typed payload. Iteration order of `Config::directives` is execution order.
//!
//! Every option field is an `Option<_>` so "unset" is distinct from
//! "explicitly set": `None` = "unset" (inherit defaults), `Some(false)` =
//! "explicitly false".

/// A parsed profile: a flat, ordered list of directives. Iteration order is
/// execution order.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Config {
    /// Source path, set by [`load`](crate::config::load) (empty when parsed
    /// from a reader).
    pub path: String,
    /// Directives in declaration (= execution) order.
    pub directives: Vec<Directive>,
}

/// One entry in a profile's ordered directive list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    /// The directive payload (exactly one kind).
    pub kind: DirectiveKind,
    /// 1-based YAML line where this directive starts; 0 if unknown. Used for
    /// error messages.
    pub line: usize,
}

/// The directive payloads. The variant name doubles as the directive's YAML
/// key (via [`DirectiveKind::name`]) so error messages can print it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectiveKind {
    /// `defaults:` — option defaults merged into subsequent directives.
    Defaults(Defaults),
    /// `link:` — target→source symlink declarations.
    Link(Link),
    /// `shell:` — ordered shell commands.
    Shell(Shell),
    /// `clean:` — directories to scan for dead symlinks.
    Clean(Clean),
    /// `create:` — directories to create.
    Create(Create),
}

impl DirectiveKind {
    /// The directive's YAML key.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            DirectiveKind::Defaults(_) => "defaults",
            DirectiveKind::Link(_) => "link",
            DirectiveKind::Shell(_) => "shell",
            DirectiveKind::Clean(_) => "clean",
            DirectiveKind::Create(_) => "create",
        }
    }
}

/// Option defaults applied to subsequent directives. A `None` section means
/// "no defaults set for that directive".
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Defaults {
    /// Defaults for `link:` options.
    pub link: Option<LinkOptions>,
    /// Defaults for `shell:` options.
    pub shell: Option<ShellOptions>,
    /// Defaults for `clean:` options.
    pub clean: Option<CleanOptions>,
}

/// Per-link options. Scalar fields are `Option<_>` so "unset" is distinct from
/// "explicitly false/empty" when merging defaults; `exclude` is a plain `Vec`
/// where empty means "unset".
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LinkOptions {
    /// Source path relative to the base dir (or absolute).
    pub path: Option<String>,
    /// Create the target's parent directory if missing.
    pub create: Option<bool>,
    /// Replace a symlink that points elsewhere.
    pub relink: Option<bool>,
    /// Alias for `relink` (dotbot compatibility): replace a symlink pointing
    /// elsewhere. Non-symlink targets are always backed up and replaced
    /// regardless of this flag.
    pub force: Option<bool>,
    /// Store a relative path in the symlink.
    pub relative: Option<bool>,
    /// Treat `path` as a glob (reserved; currently errors).
    pub glob: Option<bool>,
    /// Skip silently when `path` does not exist.
    pub ignore_missing: Option<bool>,
    /// Reserved for forward-compat; dfm always backs up.
    pub backup: Option<bool>,
    /// `"symlink"` (default) or `"hardlink"` (reserved).
    pub link_type: Option<String>,
    /// Resolve symlinks in `path` before linking.
    pub canonicalize: Option<bool>,
    /// Prefix prepended to each link's target when expanding globs.
    pub prefix: Option<String>,
    /// Glob patterns to skip when `glob: true`.
    pub exclude: Vec<String>,
}

/// One target→source pair inside a `link:` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkEntry {
    /// The path created on the filesystem, e.g. `~/.config/nvim`.
    pub target: String,
    /// Per-entry options, overriding any defaults.
    pub options: LinkOptions,
}

/// The parsed `link:` directive, preserving target order.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Link {
    /// Entries in declaration order.
    pub entries: Vec<LinkEntry>,
}

/// Per-shell-command flags.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ShellOptions {
    /// Inherit stdin from dfm.
    pub stdin: Option<bool>,
    /// Stream stdout.
    pub stdout: Option<bool>,
    /// Stream stderr.
    pub stderr: Option<bool>,
    /// Suppress the name line.
    pub quiet: Option<bool>,
}

/// One shell command. `command` is required; the rest override defaults.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ShellEntry {
    /// The command text (from the `script:` key).
    pub command: String,
    /// Human-readable label (from the `name:` key).
    pub description: String,
    /// Per-entry option overrides.
    pub options: ShellOptions,
}

/// The parsed `shell:` directive.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Shell {
    /// Commands in declaration order.
    pub entries: Vec<ShellEntry>,
}

/// Per-target clean flags.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CleanOptions {
    /// Remove dead symlinks even when they point outside the base directory.
    /// Bounded for safety: the scanned target must resolve under `$HOME`, and
    /// recursion depth is capped.
    pub force: Option<bool>,
    /// Recurse into subdirectories.
    pub recursive: Option<bool>,
}

/// One directory to scan for dead symlinks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanEntry {
    /// The directory to scan.
    pub target: String,
    /// Per-entry options.
    pub options: CleanOptions,
}

/// The parsed `clean:` directive.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Clean {
    /// Entries in declaration order.
    pub entries: Vec<CleanEntry>,
}

/// One path to `mkdir`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateEntry {
    /// The directory path.
    pub path: String,
    /// File mode; `None` = default (`0o777`).
    pub mode: Option<u32>,
}

/// The parsed `create:` directive.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Create {
    /// Entries in declaration order.
    pub entries: Vec<CreateEntry>,
}
