# dfm

Standalone, single-binary dotfiles manager. Profile format is inspired by [dotbot](https://github.com/anishathalye/dotbot) but `dfm` is its own thing — **no** Python runtime, **no** git submodule.

## Install

**macOS / Linux (Homebrew)**

```sh
brew install bitcldr/tap/dfm
```

**Prebuilt binary**

Download the archive for your platform from the [latest release](https://github.com/bitcldr/dfm/releases/latest) (darwin/linux × x86_64/arm64), extract it, and put `dfm` on your `PATH`.

**From source**

```sh
cargo install --git https://github.com/bitcldr/dfm
```

## Quick start

```sh
dfm apply <profile>...
```

Profiles are read from `./profiles/<name>.conf.yaml` relative to the base dir (`-C <dir>`, default cwd).

## Commands

| Command                  | Purpose                                  |
| ------------------------ | ---------------------------------------- |
| `dfm apply <profile>...` | Apply one or more profiles in order      |
| `dfm apply --dry-run`    | Report planned changes without writing   |
| `dfm apply --no-shell`   | Apply links/dirs/cleans, skip `shell:`   |
| `dfm diff <profile>`     | Show planned changes, no writes          |
| `dfm doctor`             | Verify installed symlinks still resolve  |
| `dfm status`             | Show last applied profiles and timestamp |
| `dfm list`               | List profiles found in `./profiles/`     |
| `dfm completion <shell>` | Output shell completion script           |

Global flags: `-C <dir>`, `-c <path>`, `--verbose` (debug tracing), `-q`/`--quiet` (suppress progress; warnings always visible), `--color=auto|always|never` (default: `auto`; also respects `NO_COLOR` env var and `CLICOLOR_FORCE=1`).

Progress output (links created, shell commands, apply summary) goes to **stderr**. Data output (profile list, completion scripts, `dfm status`) goes to **stdout** and is safe to pipe. Warnings always appear on stderr regardless of `--quiet`. Use `--quiet` to suppress progress output, `--verbose` for debug tracing.

## Shell completion

```sh
# fish
dfm completion fish > ~/.config/fish/completions/dfm.fish

# bash — add to ~/.bashrc
source <(dfm completion bash)

# zsh — add to ~/.zshrc
source <(dfm completion zsh)
```

Completes subcommands, flags, and profile names (via `dfm list`). Set `$DFM_DIR` to point completions at a non-cwd base directory.

## Supported directives

Directives: `defaults`, `link`, `shell`, `clean`, `create`. Unknown directives are rejected.

- Non-symlink targets are backed up to `~/.dotfiles-backup/<timestamp>/` instead of failing
- `shell` entries use `name:` + `script:` (multiline blocks supported)

Full reference: [docs/yaml-spec.md](docs/yaml-spec.md).

## Non-goals

Templating, secret management, package installation, plugins, profile inheritance.

## License

MIT
