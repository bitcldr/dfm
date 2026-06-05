//! `dfm status` — show the most recently applied profiles and when.

use chrono::{Local, Utc};

use crate::state;

use super::Ctx;
use super::profiles::CliResult;

/// Run `dfm status`.
pub fn run(ctx: &mut Ctx) -> CliResult {
    let Some(s) = state::load()? else {
        ctx.io
            .status_empty("no profiles have been applied on this machine yet");
        return Ok(());
    };

    let path = state::path()?;
    ctx.io
        .status_line("State file:  ", &path.display().to_string());
    ctx.io
        .status_line("Last applied:", &s.last_applied.join(" "));
    let applied_local = s.applied_at.with_timezone(&Local);
    ctx.io.status_line_with_meta(
        "Applied at:  ",
        &applied_local.to_rfc3339(),
        &format!("({} ago)", human_since(s.applied_at)),
    );
    ctx.io
        .status_line("Links:       ", &s.links.len().to_string());

    Ok(())
}

/// A coarse human-readable age like "3h" or "2d". Deliberately coarse — status
/// is glanceable, not a stopwatch.
fn human_since(t: chrono::DateTime<Utc>) -> String {
    let secs = (Utc::now() - t).num_seconds().max(0);
    match secs {
        s if s < 60 => "just now".to_string(),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86400),
    }
}
