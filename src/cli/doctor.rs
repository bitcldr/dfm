//! `dfm doctor` — verify previously applied symlinks still resolve.

use crate::state;

use super::Ctx;
use super::profiles::CliResult;

/// Run `dfm doctor`.
///
/// Returns an error (non-zero exit) when any link is missing, not a symlink, or
/// pointing at the wrong source — making `dfm doctor` scriptable in CI.
pub fn run(ctx: &mut Ctx) -> CliResult {
    let Some(s) = state::load()? else {
        ctx.io
            .doctor_fail("no applied state found — run `dfm apply` first");
        return Ok(());
    };

    let mut problems = Vec::new();
    let mut ok = 0u32;

    for l in &s.links {
        match std::fs::read_link(&l.target) {
            Ok(dest) => {
                let dest = dest.to_string_lossy();
                if dest == l.source {
                    ok += 1;
                } else {
                    problems.push(format!(
                        "drifted: {} → {dest} (want {})",
                        l.target, l.source
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                problems.push(format!("missing: {}", l.target));
            }
            Err(e) => {
                problems.push(format!("not a symlink: {} ({e})", l.target));
            }
        }
    }

    ctx.io
        .doctor_done(ok, u32::try_from(problems.len()).unwrap_or(u32::MAX));
    for p in &problems {
        ctx.io.doctor_item(p);
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{} link(s) need attention", problems.len()).into())
    }
}
