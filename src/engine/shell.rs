//! The `shell:` directive executor.
//!
//! Each command runs under `/bin/sh -c` with cwd = base dir. A single failing
//! command is recorded but does not abort the directive — subsequent commands
//! still run. `set -e` is not injected; the script is handed to the shell
//! verbatim.

use std::process::{Command, Stdio};

use crate::config::Shell;

use super::action::ActionKind;
use super::core::{Engine, Tally, bool_or, merge_shell_opts};
use super::sink::OutputSink;

impl<S: OutputSink> Engine<S> {
    /// Execute a `shell:` directive.
    pub(crate) fn run_shell(&mut self, s: &Shell, tally: &mut Tally) {
        for item in &s.entries {
            let opts = merge_shell_opts(&self.defaults.shell, &item.options);
            let quiet = bool_or(opts.quiet, false);
            self.sink.shell_cmd(&item.description, &item.command, quiet);

            log::debug!(
                "shell cmd={:?} dir={} dry_run={}",
                item.command,
                self.base_dir.display(),
                self.dry_run
            );
            self.record(
                ActionKind::ShellRun,
                item.command.clone(),
                item.description.clone(),
            );
            if self.dry_run {
                continue;
            }

            let mut cmd = Command::new("/bin/sh");
            cmd.arg("-c").arg(&item.command).current_dir(&self.base_dir);

            // Inherit the selected streams; otherwise the child's are dropped.
            cmd.stdin(if bool_or(opts.stdin, false) {
                Stdio::inherit()
            } else {
                Stdio::null()
            });
            cmd.stdout(if !quiet && bool_or(opts.stdout, false) {
                Stdio::inherit()
            } else {
                Stdio::null()
            });
            cmd.stderr(if !quiet && bool_or(opts.stderr, false) {
                Stdio::inherit()
            } else {
                Stdio::null()
            });

            tally.shell_run += 1;
            match cmd.status() {
                Ok(status) if status.success() => {}
                Ok(status) => {
                    tally.shell_failed += 1;
                    log::warn!("command failed [{}]: exit {status}", item.command);
                }
                Err(e) => {
                    tally.shell_failed += 1;
                    log::warn!("command failed [{}]: {e}", item.command);
                }
            }
        }
    }
}
