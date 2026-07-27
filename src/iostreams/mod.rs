//! The three standard I/O streams for a command invocation, plus styled print
//! helpers used by every subcommand.
//!
//! Stream routing:
//!
//! - `out` receives data output (lists, completion scripts) — safe to pipe.
//! - `err_out` receives both progress and diagnostics. Progress is gated by
//!   the quiet flag; diagnostics (doctor, warnings) always write through.
//!
//! Writers are **raw** (`Box<dyn Write + Send>`), not `anstream::AutoStream` —
//! color is decided once via [`ColorPolicy`] and applied through [`paint`].
//! This keeps the streams fd-extractable for subprocess wiring later.

mod color;
mod style;

pub use color::{ColorEnv, ColorPolicy, SystemColorEnv};

use std::borrow::Cow;
use std::io::{self, IsTerminal, Write};

use style::{BOLD, BOLD_CYAN, BOLD_GREEN, BOLD_WHITE, DIM, HI_RED, WHITE, YELLOW, arrow, paint};

/// Totals for the apply/dry-run summary line. Decoupled from the engine's
/// `Tally` to avoid a cross-module dependency.
#[derive(Debug, Default, Clone, Copy)]
pub struct ApplyResult {
    /// Links already correct.
    pub links_ok: u32,
    /// Links newly created.
    pub created: u32,
    /// Stale links replaced.
    pub relinked: u32,
    /// Non-symlink targets backed up.
    pub backed_up: u32,
    /// Shell commands run.
    pub shell_run: u32,
    /// Shell commands that failed.
    pub shell_failed: u32,
    /// Dead links cleaned.
    pub cleaned: u32,
    /// Directories created.
    pub dirs: u32,
}

/// Holds the standard streams for one command invocation.
pub struct IoStreams {
    /// Data output (stdout-like).
    out: Box<dyn Write + Send>,
    /// Progress + diagnostics output (stderr-like).
    err_out: Box<dyn Write + Send>,
    /// Suppress progress; diagnostics are unaffected.
    quiet: bool,
    /// Whether `out` is color-capable.
    out_color: bool,
    /// Whether `err_out` is color-capable.
    err_color: bool,
}

impl IoStreams {
    /// Wire to the real process streams with TTY- and env-aware color
    /// detection under the given policy.
    #[must_use]
    pub fn new(policy: ColorPolicy) -> Self {
        let env = SystemColorEnv;
        let out_color = color::resolve(policy, &env, &io::stdout());
        let err_color = color::resolve(policy, &env, &io::stderr());
        Self {
            out: Box::new(io::stdout()),
            err_out: Box::new(io::stderr()),
            quiet: false,
            out_color,
            err_color,
        }
    }

    /// Build with explicit writers and colors (tests). Quiet defaults off.
    pub fn with_writers(
        out: impl Write + Send + 'static,
        err_out: impl Write + Send + 'static,
        out_color: bool,
        err_color: bool,
    ) -> Self {
        Self {
            out: Box::new(out),
            err_out: Box::new(err_out),
            quiet: false,
            out_color,
            err_color,
        }
    }

    /// A sink that silently drops all output (used when no streams are
    /// injected, e.g. in engine tests).
    #[must_use]
    pub fn discard() -> Self {
        Self {
            out: Box::new(io::sink()),
            err_out: Box::new(io::sink()),
            quiet: false,
            out_color: false,
            err_color: false,
        }
    }

    /// Mark the streams quiet: progress helpers become no-ops; diagnostics and
    /// data output are unaffected.
    pub fn set_quiet(&mut self, quiet: bool) {
        self.quiet = quiet;
    }

    /// Whether quiet mode is active.
    #[must_use]
    pub fn is_quiet(&self) -> bool {
        self.quiet
    }

    /// Whether `err_out` is color-capable (used to gate logger coloring).
    #[must_use]
    pub fn err_color_enabled(&self) -> bool {
        self.err_color
    }

    /// Whether `out` is color-capable.
    #[must_use]
    pub fn out_color_enabled(&self) -> bool {
        self.out_color
    }

    // ── low-level write helpers ────────────────────────────────────────────

    /// Write a line to `err_out` unless quiet. All progress helpers route
    /// through here so quiet suppression lives in one place. Write errors are
    /// swallowed (fire-and-forget): progress output is non-essential.
    fn progress(&mut self, line: &str) {
        if self.quiet {
            return;
        }
        let _ = self.err_out.write_all(line.as_bytes());
    }

    /// Write a line to `err_out` regardless of quiet (diagnostics).
    fn diag(&mut self, line: &str) {
        let _ = self.err_out.write_all(line.as_bytes());
    }

    /// Write a line to `out` (data). Never gated by quiet.
    fn data(&mut self, line: &str) {
        let _ = self.out.write_all(line.as_bytes());
    }

    /// Write raw text to the data stream (stdout) verbatim. Used for output
    /// like completion scripts. Never gated by quiet.
    pub fn write_out(&mut self, text: &str) {
        let _ = self.out.write_all(text.as_bytes());
    }

    // ── engine progress (→ err_out, quiet-gated) ───────────────────────────

    /// Report that a new symlink `from -> to` was created.
    pub fn linked(&mut self, from: &str, to: &str) {
        let e = self.err_color;
        self.progress(&format!(
            "{} {} {} {}\n",
            paint(BOLD_GREEN, "Linked", e),
            paint(WHITE, from, e),
            paint(DIM, arrow(), e),
            paint(DIM, to, e)
        ));
    }

    /// Report that a stale symlink was replaced with `from -> to`.
    pub fn relinked(&mut self, from: &str, to: &str) {
        let e = self.err_color;
        self.progress(&format!(
            "{} {} {} {}\n",
            paint(YELLOW, "Relinked", e),
            paint(WHITE, from, e),
            paint(DIM, arrow(), e),
            paint(DIM, to, e)
        ));
    }

    /// Report that an existing symlink already points at the correct target.
    pub fn link_ok(&mut self, from: &str, to: &str) {
        let e = self.err_color;
        self.progress(&format!(
            "{} {} {} {}\n",
            paint(BOLD, "Link exists", e),
            paint(DIM, from, e),
            paint(WHITE, arrow(), e),
            paint(DIM, to, e)
        ));
    }

    /// Report that a pre-existing file was moved `from -> to` before linking.
    pub fn backed_up(&mut self, from: &str, to: &str) {
        let e = self.err_color;
        self.progress(&format!(
            "{} {} {} {}\n",
            paint(YELLOW, "backed up", e),
            paint(WHITE, from, e),
            paint(DIM, arrow(), e),
            paint(DIM, to, e)
        ));
    }

    /// Report that a dangling symlink `from -> to` was removed.
    pub fn removed_dead_link(&mut self, from: &str, to: &str) {
        let e = self.err_color;
        self.progress(&format!(
            "{} {} {} {}\n",
            paint(YELLOW, "Removed", e),
            paint(WHITE, from, e),
            paint(DIM, arrow(), e),
            paint(DIM, to, e)
        ));
    }

    /// Report that a create target already exists and was skipped.
    pub fn path_exists(&mut self, path: &str) {
        let e = self.err_color;
        self.progress(&format!(
            "{} {}\n",
            paint(BOLD, "Path exists", e),
            paint(DIM, path, e)
        ));
    }

    /// Report that a directory was created.
    pub fn created(&mut self, path: &str) {
        let e = self.err_color;
        self.progress(&format!(
            "{} {}\n",
            paint(BOLD_GREEN, "Created", e),
            paint(WHITE, path, e)
        ));
    }

    /// Report that a profile is being applied.
    pub fn applying(&mut self, path: &str) {
        let e = self.err_color;
        self.progress(&format!(
            "{} {}\n",
            paint(BOLD, "Applying", e),
            paint(DIM, path, e)
        ));
    }

    /// Report that a profile would be applied (dry run).
    pub fn would_apply(&mut self, path: &str) {
        let e = self.err_color;
        self.progress(&format!(
            "{} {}\n",
            paint(BOLD_CYAN, "Would apply", e),
            paint(WHITE, path, e)
        ));
    }

    /// Print a shell directive entry. `quiet_opt` mirrors the directive's
    /// `quiet` field (distinct from stream-level quiet).
    pub fn shell_cmd(&mut self, description: &str, command: &str, quiet_opt: bool) {
        let e = self.err_color;
        let line = if quiet_opt && !description.is_empty() {
            format!("{}\n", paint(BOLD, description, e))
        } else if !description.is_empty() {
            format!(
                "{} {}\n",
                paint(BOLD, description, e),
                paint(DIM, &format!("[{command}]"), e)
            )
        } else {
            format!("{}\n", paint(WHITE, command, e))
        };
        self.progress(&line);
    }

    /// Print the apply/dry-run summary line.
    pub fn done(&mut self, dry_run: bool, r: ApplyResult) {
        let e = self.err_color;
        let verb = if dry_run {
            paint(BOLD_CYAN, "dry-run", e)
        } else {
            paint(BOLD_WHITE, "Done:", e)
        };
        let sep = ", ";
        let line = format!(
            "{} {}{sep}{}{sep}{}{sep}{}{sep}{}{sep}{}{sep}{}\n",
            verb,
            stat_count(r.links_ok, "ok", e),
            stat_count(r.created, "created", e),
            stat_count(r.relinked, "relinked", e),
            stat_count(r.backed_up, "backed up", e),
            shell_count(r.shell_run, r.shell_failed, e),
            stat_count(r.cleaned, "cleaned", e),
            stat_count(r.dirs, "dirs", e),
        );
        self.progress(&line);
    }

    // ── diagnostics (→ err_out, always visible) ────────────────────────────

    /// Write a plain diagnostic message (e.g. "no state found") to `err_out`.
    pub fn doctor_fail(&mut self, msg: &str) {
        let e = self.err_color;
        self.diag(&format!("{}\n", paint(WHITE, msg, e)));
    }

    /// Print the inline doctor summary line.
    pub fn doctor_done(&mut self, ok: u32, problems: u32) {
        let e = self.err_color;
        self.diag(&format!(
            "{} {}, {}\n",
            paint(BOLD_WHITE, "Doctor:", e),
            stat_count(ok, "ok", e),
            problem_count(problems, e),
        ));
    }

    /// Write one indented problem line under a doctor summary.
    pub fn doctor_item(&mut self, problem: &str) {
        let e = self.err_color;
        self.diag(&format!("  {}\n", paint(DIM, problem, e)));
    }

    // ── status output (→ out / stdout) ─────────────────────────────────────

    /// Write a `label value` pair to stdout.
    pub fn status_line(&mut self, label: &str, value: &str) {
        let o = self.out_color;
        self.data(&format!(
            "{} {}\n",
            paint(BOLD, label, o),
            paint(DIM, value, o)
        ));
    }

    /// Write a `label value (meta)` triple to stdout.
    pub fn status_line_with_meta(&mut self, label: &str, value: &str, meta: &str) {
        let o = self.out_color;
        self.data(&format!(
            "{} {} {}\n",
            paint(BOLD, label, o),
            paint(DIM, value, o),
            paint(WHITE, meta, o)
        ));
    }

    /// Write a dim placeholder when there is no status to show.
    pub fn status_empty(&mut self, msg: &str) {
        let o = self.out_color;
        self.data(&format!("{}\n", paint(DIM, msg, o)));
    }

    // ── diff output (→ out / stdout) ───────────────────────────────────────

    /// Write a diff section header with an item count.
    pub fn diff_header(&mut self, header: &str, n: usize) {
        let o = self.out_color;
        self.data(&format!(
            "{} {}\n",
            paint(BOLD_WHITE, header, o),
            paint(DIM, &format!("({n})"), o)
        ));
    }

    /// Write one indented action line in the diff output.
    pub fn diff_action(&mut self, text: &str) {
        let o = self.out_color;
        self.data(&format!("  {}\n", paint(WHITE, text, o)));
    }

    /// Write a "no changes" placeholder when the diff is empty.
    pub fn diff_empty(&mut self) {
        let o = self.out_color;
        self.data(&format!("{}\n", paint(DIM, "No changes", o)));
    }

    // ── list output (→ out / stdout) ───────────────────────────────────────

    /// Print a styled profile list. Empty → "No profiles found".
    pub fn profile_list(&mut self, names: &[String]) {
        let o = self.out_color;
        if names.is_empty() {
            self.data(&format!("{}\n", paint(DIM, "No profiles found", o)));
            return;
        }
        let sep = format!("{} ", paint(WHITE, ",", o));
        let parts: Vec<Cow<'_, str>> = names.iter().map(|n| paint(DIM, n, o)).collect();
        self.data(&format!(
            "{} {}\n",
            paint(WHITE, "Profiles:", o),
            parts.join(&sep)
        ));
    }
}

// ── internal count formatters ──────────────────────────────────────────────

fn stat_count(n: u32, label: &str, color_on: bool) -> String {
    format!(
        "{} {}",
        paint(WHITE, &n.to_string(), color_on),
        paint(DIM, label, color_on)
    )
}

fn problem_count(n: u32, color_on: bool) -> String {
    let num_style = if n > 0 { HI_RED } else { WHITE };
    format!(
        "{} {}",
        paint(num_style, &n.to_string(), color_on),
        paint(DIM, "problems", color_on)
    )
}

fn shell_count(run: u32, failed: u32, color_on: bool) -> String {
    let mut s = format!(
        "{} {}",
        paint(WHITE, &run.to_string(), color_on),
        paint(DIM, "shell", color_on)
    );
    if failed > 0 {
        s.push(' ');
        s.push_str(&paint(HI_RED, &format!("({failed} failed)"), color_on));
    }
    s
}

/// Whether stdout is an interactive terminal (helper for callers wiring color).
#[must_use]
pub fn stdout_is_terminal() -> bool {
    io::stdout().is_terminal()
}
