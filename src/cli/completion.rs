//! `dfm completion` — output a shell completion script.
//!
//! The scripts are maintained by hand (embedded via `include_str!`) rather
//! than generated, because they complete profile names dynamically by calling
//! `dfm list` — behavior a generated static script cannot express.

use super::Ctx;
use super::profiles::CliResult;

const BASH: &str = include_str!("completions/bash.sh");
const ZSH: &str = include_str!("completions/zsh.zsh");
const FISH: &str = include_str!("completions/fish.fish");

/// Arguments for `dfm completion`.
#[derive(Debug, clap::Args)]
pub struct CompletionArgs {
    /// The shell to emit a completion script for.
    #[arg(value_name = "shell")]
    pub shell: String,
}

/// Run `dfm completion`. The script goes to stdout (data output).
pub fn run(args: &CompletionArgs, ctx: &mut Ctx) -> CliResult {
    let script = match args.shell.to_lowercase().as_str() {
        "bash" => BASH,
        "zsh" => ZSH,
        "fish" => FISH,
        other => {
            return Err(format!("unknown shell {other:?} — supported: bash, zsh, fish").into());
        }
    };
    ctx.io.write_out(script);
    Ok(())
}
