//! `dfm apply` — apply one or more profiles to the home directory.

use chrono::Utc;

use crate::config;
use crate::engine::{Action, ActionKind, Engine, IoStreamsSink, Tally};
use crate::iostreams::ApplyResult;
use crate::state::{self, Link, State};

use super::Ctx;
use super::profiles::{CliResult, resolve_profile_paths};

/// Arguments for `dfm apply`.
#[derive(Debug, clap::Args)]
pub struct ApplyArgs {
    /// Report planned changes without writing.
    #[arg(long)]
    pub dry_run: bool,

    /// Skip `shell:` directives — apply only links, cleans, and directories.
    #[arg(long)]
    pub no_shell: bool,

    /// Profile name(s) to apply, in order.
    #[arg(value_name = "profile")]
    pub profiles: Vec<String>,
}

/// Run `dfm apply`.
pub fn run(args: &ApplyArgs, ctx: &mut Ctx) -> CliResult {
    let base_abs = ctx.base_abs()?;
    let paths = resolve_profile_paths(
        &base_abs,
        ctx.globals.config_path.as_deref(),
        &args.profiles,
    )?;

    // A single backup tag shared across the whole run.
    let backup_tag = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();

    let mut totals = Tally::default();
    let mut actions: Vec<Action> = Vec::new();

    for path in &paths {
        let cfg = config::load(path)?;

        if args.dry_run {
            ctx.io.would_apply(&path.display().to_string());
        } else {
            ctx.io.applying(&path.display().to_string());
        }

        // Scope the sink borrow so we can touch ctx.io again afterwards.
        let tally = {
            let sink = IoStreamsSink::new(&mut ctx.io);
            let mut eng = Engine::new(base_abs.clone())
                .dry_run(args.dry_run)
                .skip_shell(args.no_shell)
                .with_sink(sink);
            eng.set_backup_tag(&backup_tag);
            let tally = eng.apply(&cfg)?;
            actions.extend(eng.actions.iter().cloned());
            tally
        };
        totals = add(totals, tally);
    }

    if !args.dry_run
        && let Err(e) = state::save(&State {
            last_applied: args.profiles.clone(),
            applied_at: Utc::now(),
            links: collect_links(&actions),
        })
    {
        log::warn!("state save: {e}");
    }

    ctx.io.done(
        args.dry_run,
        ApplyResult {
            links_ok: totals.links_ok,
            created: totals.links_created,
            relinked: totals.links_relinked,
            backed_up: totals.links_backed_up,
            shell_run: totals.shell_run,
            shell_failed: totals.shell_failed,
            cleaned: totals.cleaned,
            dirs: totals.created,
        },
    );

    Ok(())
}

/// Pick out the symlinks the engine created or confirmed, so `dfm doctor` can
/// later verify them. Skipped or backed-up actions are not recorded.
fn collect_links(actions: &[Action]) -> Vec<Link> {
    actions
        .iter()
        .filter(|a| {
            matches!(
                a.kind,
                ActionKind::LinkCreate | ActionKind::LinkRelink | ActionKind::LinkExists
            )
        })
        .map(|a| Link {
            target: a.from.clone(),
            source: a.to.clone(),
        })
        .collect()
}

/// Sum two tallies field-by-field so totals can span profiles.
fn add(a: Tally, b: Tally) -> Tally {
    Tally {
        links_ok: a.links_ok + b.links_ok,
        links_created: a.links_created + b.links_created,
        links_relinked: a.links_relinked + b.links_relinked,
        links_backed_up: a.links_backed_up + b.links_backed_up,
        shell_run: a.shell_run + b.shell_run,
        shell_failed: a.shell_failed + b.shell_failed,
        cleaned: a.cleaned + b.cleaned,
        created: a.created + b.created,
    }
}
