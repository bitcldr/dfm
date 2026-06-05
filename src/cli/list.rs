//! `dfm list` — enumerate profile files under `<base>/profiles/`.

use super::Ctx;
use super::profiles::CliResult;

/// Run `dfm list`.
pub fn run(ctx: &mut Ctx) -> CliResult {
    let base_abs = ctx.base_abs()?;
    let dir = base_abs.join("profiles");

    let entries =
        std::fs::read_dir(&dir).map_err(|e| format!("list: read {}: {e}", dir.display()))?;

    let mut names = Vec::new();
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(stem) = name.strip_suffix(".conf.yaml") {
            names.push(stem.to_string());
        }
    }
    names.sort();

    ctx.io.profile_list(&names);
    Ok(())
}
