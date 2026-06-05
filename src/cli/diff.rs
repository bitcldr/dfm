//! `dfm diff` — show the planned filesystem changes without writing.

use crate::config;
use crate::engine::{Action, ActionKind, Engine};

use super::Ctx;
use super::profiles::{CliResult, resolve_profile_paths};

/// Arguments for `dfm diff`.
#[derive(Debug, clap::Args)]
pub struct DiffArgs {
    /// Profile name(s) to diff.
    #[arg(value_name = "profile")]
    pub profiles: Vec<String>,
}

/// Run `dfm diff`.
pub fn run(args: &DiffArgs, ctx: &mut Ctx) -> CliResult {
    let base_abs = ctx.base_abs()?;
    let paths = resolve_profile_paths(
        &base_abs,
        ctx.globals.config_path.as_deref(),
        &args.profiles,
    )?;

    let mut actions: Vec<Action> = Vec::new();
    for path in &paths {
        let cfg = config::load(path)?;
        let mut eng = Engine::new(base_abs.clone()).dry_run(true);
        eng.apply(&cfg)?;
        actions.extend(eng.actions.iter().cloned());
    }

    print_diff(ctx, &actions);
    Ok(())
}

/// Group actions by kind and print each non-empty group under a header.
fn print_diff(ctx: &mut Ctx, actions: &[Action]) {
    const ORDER: [ActionKind; 9] = [
        ActionKind::LinkCreate,
        ActionKind::LinkRelink,
        ActionKind::LinkBackup,
        ActionKind::LinkSkip,
        ActionKind::LinkExists,
        ActionKind::CreateDir,
        ActionKind::CreateExists,
        ActionKind::CleanRemove,
        ActionKind::ShellRun,
    ];

    let mut empty = true;
    for kind in ORDER {
        let group: Vec<&Action> = actions.iter().filter(|a| a.kind == kind).collect();
        if group.is_empty() {
            continue;
        }
        empty = false;
        ctx.io.diff_header(header_for(kind), group.len());
        for a in group {
            ctx.io.diff_action(&format_action(a));
        }
    }

    if empty {
        ctx.io.diff_empty();
    }
}

fn header_for(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::LinkCreate => "+ Links to create",
        ActionKind::LinkRelink => "~ Links to relink",
        ActionKind::LinkBackup => "! Non-symlink targets to back up",
        ActionKind::LinkSkip => "? Links blocked by conflict (need relink/force)",
        ActionKind::LinkExists => "= Links already correct",
        ActionKind::CreateDir => "+ Directories to create",
        ActionKind::CreateExists => "= Directories already present",
        ActionKind::CleanRemove => "- Dead links to remove",
        ActionKind::ShellRun => "$ Shell commands to run",
    }
}

fn format_action(a: &Action) -> String {
    match a.kind {
        ActionKind::ShellRun => {
            if a.to.is_empty() {
                a.from.clone()
            } else {
                format!("{}  [{}]", a.to, a.from)
            }
        }
        ActionKind::CreateDir | ActionKind::CreateExists => a.from.clone(),
        _ => {
            if a.to.is_empty() {
                a.from.clone()
            } else {
                format!("{} -> {}", a.from, a.to)
            }
        }
    }
}
