//! Command-line interface: argument parsing, subcommands, and dispatch.

mod apply;
mod completion;
mod diff;
mod doctor;
mod list;
mod profiles;
mod status;

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::iostreams::{ColorPolicy, IoStreams};

/// A standalone, single-binary dotfiles manager.
#[derive(Debug, Parser)]
#[command(
    name = "dfm",
    version = crate::version::REVISION,
    about = "Standalone single-binary dotfiles manager"
)]
pub struct Cli {
    /// Global flags shared by every subcommand.
    #[command(flatten)]
    pub globals: GlobalArgs,

    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Flags shared across all subcommands.
#[derive(Debug, clap::Args)]
pub struct GlobalArgs {
    /// Base directory for resolving profiles and sources.
    #[arg(short = 'C', long = "dir", default_value = ".", global = true)]
    pub base_dir: PathBuf,

    /// Explicit config path (overrides profile name lookup).
    #[arg(short = 'c', long = "config", global = true)]
    pub config_path: Option<PathBuf>,

    /// Enable verbose (debug) logging.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Suppress progress output (warnings and errors still shown).
    #[arg(short = 'q', long, global = true)]
    pub quiet: bool,

    /// Colorize output.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, global = true)]
    pub color: ColorChoice,
}

/// The `--color` flag values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum ColorChoice {
    /// Decide from env and TTY detection.
    Auto,
    /// Always colorize.
    Always,
    /// Never colorize.
    Never,
}

impl From<ColorChoice> for ColorPolicy {
    fn from(c: ColorChoice) -> Self {
        match c {
            ColorChoice::Auto => ColorPolicy::Auto,
            ColorChoice::Always => ColorPolicy::Always,
            ColorChoice::Never => ColorPolicy::Never,
        }
    }
}

/// The available subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Apply one or more profiles.
    Apply(apply::ApplyArgs),
    /// Show planned changes without writing.
    Diff(diff::DiffArgs),
    /// Verify installed symlinks still resolve.
    Doctor,
    /// Show last applied profiles.
    Status,
    /// List available profiles.
    List,
    /// Output a shell completion script.
    Completion(completion::CompletionArgs),
}

/// Shared context passed to each subcommand's `run`.
pub struct Ctx {
    /// Resolved global flags.
    pub globals: GlobalArgs,
    /// Output streams.
    pub io: IoStreams,
}

impl Ctx {
    /// The base directory as an absolute path.
    pub fn base_abs(&self) -> std::io::Result<PathBuf> {
        std::path::absolute(&self.globals.base_dir)
    }
}

/// Parse arguments and run. Returns the process exit code.
///
/// `--help`/`--version` exit 0; usage/parse errors and command failures exit 1
/// (matching the original CLI rather than clap's default of 2).
#[must_use]
pub fn run(args: impl IntoIterator<Item = String>) -> i32 {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            // Only explicit --help / --version requests exit 0. A missing
            // subcommand still prints help text but is an error (exit 1).
            let is_info = matches!(
                e.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            );
            let _ = e.print();
            return i32::from(!is_info);
        }
    };

    if cli.globals.verbose && cli.globals.quiet {
        eprintln!("--verbose and --quiet are mutually exclusive");
        return 1;
    }

    let mut io = IoStreams::new(cli.globals.color.into());
    if cli.globals.quiet {
        io.set_quiet(true);
    }

    let mut ctx = Ctx {
        globals: cli.globals,
        io,
    };

    let result = match cli.command {
        Command::Apply(args) => apply::run(&args, &mut ctx),
        Command::Diff(args) => diff::run(&args, &mut ctx),
        Command::Doctor => doctor::run(&mut ctx),
        Command::Status => status::run(&mut ctx),
        Command::List => list::run(&mut ctx),
        Command::Completion(args) => completion::run(&args, &mut ctx),
    };

    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("dfm: {e:#}");
            1
        }
    }
}
