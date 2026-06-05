# dfm profile YAML specification

This document is the normative reference for the profile YAML consumed by
`dfm apply` / `dfm diff`.

Anything the parser doesn't recognise is an error, not a warning — typos
fail fast instead of silently doing nothing.

## File layout

A profile is a single YAML document whose root is a **mapping**. Each
key is a directive name; the value is the directive body. Order is
preserved (yaml.v3 maintains insertion order) and equal to execution order.

```yaml
# profiles/base.conf.yaml
defaults:
  link:
    create: true
    relink: true

clean:
  - "~"

link:
  ~/.zshrc: zshrc.zsh
  ~/.config/nvim:
    path: config/nvim
    force: true

shell:
  - name: Installing submodules
    script: git submodule update --init
```

Parsing rules:

1. **Top level must be a mapping.** The root of the document must be a
   YAML mapping (key: value pairs, no leading `-`). A sequence or scalar
   at the root is rejected.

2. **Each key is a directive name.** Valid keys are `defaults`, `link`,
   `shell`, `clean`, `create`. Each may appear at most once per file —
   YAML mappings don't allow duplicate keys. If you need two shell phases,
   merge their entries into a single `shell:` block.

3. **Unknown keys are errors, not warnings.** There is no plugin mechanism
   and no fallback behaviour. A typo like `lnik:` fails immediately with a
   line/column reference rather than silently doing nothing.

4. **Execution order follows declaration order.** Put `defaults:` before
   the directives it should affect.

## `defaults`

Merges option defaults into **subsequent** directives in the same file.
Defaults from one profile do **not** leak into another profile, even in
the same `apply` invocation.

```yaml
defaults:
  link:
    create: true
    relink: true
    force: false
  shell:
    stdout: false
    stderr: true
    quiet: false
  clean:
    force: false
    recursive: false
```

Recognised sub-keys: `link`, `shell`, `clean`. `create` is accepted but
currently ignored (mode is per-entry in practice).

## `link`

Maps `target → source`. `target` is the path created on the filesystem
(supports `~` and `$VAR`); `source` is either a string (shorthand for
`{ path: ... }`) or a mapping of per-link options.

```yaml
link:
  # string form
  ~/.zshrc: zshrc.zsh

  # map form with options
  ~/.config/nvim:
    path: config/nvim
    create: true
    relink: true
    force: false
```

### Link options

| Key              | Type                    | Meaning                                                                                                                          |
| ---------------- | ----------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `path`           | string                  | Source path relative to the profile dir (or absolute). Defaults to the target's basename stripped of a leading dot when omitted. |
| `create`         | bool                    | Create the target's parent directory if missing. Default `false` (set globally via `defaults`).                                  |
| `relink`         | bool                    | If target is a symlink pointing elsewhere, unlink and recreate. Default `false`.                                                 |
| `force`          | bool                    | If target is a non-symlink, back it up and replace. `dfm` always backs up; this flag is accepted but is currently a no-op.       |
| `relative`       | bool                    | Store a relative path in the symlink instead of an absolute one. Default `false`.                                                |
| `glob`           | bool                    | Treat `path` as a glob and create one symlink per match. **Reserved** — glob support is deferred; using it currently errors.     |
| `ignore-missing` | bool                    | If `path` doesn't exist, skip silently. Default `false`.                                                                         |
| `backup`         | bool                    | Reserved for forward-compat; `dfm` always backs up.                                                                              |
| `type`           | `symlink` \| `hardlink` | Default `symlink`. Hardlinks are reserved.                                                                                       |
| `canonicalize`   | bool                    | Resolve symlinks in `path` before linking. Alias: `canonicalize-path`.                                                           |
| `prefix`         | string                  | Prefix prepended to each link's target when expanding globs.                                                                     |
| `exclude`        | list<string>            | Glob patterns to skip when `glob: true`.                                                                                         |

### Link semantics (per target)

1. Resolve `source` relative to the base directory; expand `~` / `$VAR`
   in `target`.
2. If the target is already a symlink pointing at the correct source →
   no-op, recorded as `link-ok`.
3. If the target is a symlink to a different place:
   - `relink: true` → replace it (`relink` action).
   - otherwise → `skip`, with a warning.
4. If the target exists and is **not** a symlink (regular file or
   directory) → move to `~/.dotfiles-backup/<rfc3339>/<path>` and
   replace (`backup` + `link` actions).
5. If the parent directory is missing and `create: true` → create it.

> `dfm` always backs up before replacing a non-symlink target. The backup
> is reversible, making `apply` idempotent by default.

## `shell`

Runs scripts under `/bin/sh -c` in the base directory, in order. Each
entry is a mapping with `name:` + `script:`. There is exactly one shape —
no scalar or list shorthand.

```yaml
shell:
  - name: 📦 install tools
    script: mise install

  - name: 🔧 rebuild assets
    script: |
      ls -laR /tmp
      du -hcs /srv
      cat /tmp/conf.yml
      echo all good, 123
    quiet: true
```

### Shell options

| Key      | Type   | Meaning                                                                                                  |
| -------- | ------ | -------------------------------------------------------------------------------------------------------- |
| `name`   | string | Human-readable label shown in progress output. Required.                                                 |
| `script` | string | The command text. Short commands go inline; multiline scripts use a YAML literal block (`\|`). Required. |
| `stdin`  | bool   | Inherit stdin from `dfm`. Default `false`.                                                               |
| `stdout` | bool   | Stream stdout. Default `false`.                                                                          |
| `stderr` | bool   | Stream stderr. Default `true`.                                                                           |
| `quiet`  | bool   | Suppress the name line. Default `false`.                                                                 |

> **Script semantics.** The `script` value is handed verbatim to
> `sh -c`, newlines and all. **`set -e` is not injected** — if you want
> a multiline script to stop at the first failure, add `set -eu` at the
> top yourself. A script exiting non-zero is recorded as a failure in
> the tally but does **not** abort the run; subsequent directives still
> execute.
>
> **Skipping shell.** Run `dfm apply --no-shell` to apply only the
> `link`, `clean`, and `create` directives and skip every `shell:` block —
> useful when you want the symlinks without running install scripts.
> `dfm diff` never executes shell commands (it only reports them).

## `clean`

Removes **dead symlinks** under the given directories when the symlink
points back into the base directory. Live symlinks are never touched;
symlinks pointing outside the base dir are never touched.

Scalar-list form:

```yaml
clean:
  - "~"
  - "~/.config"
```

Map form with options:

```yaml
clean:
  "~":
    force: false
    recursive: false
```

### Clean options

| Key         | Type | Meaning                                       |
| ----------- | ---- | --------------------------------------------- |
| `force`     | bool | Reserved. Default `false`.                    |
| `recursive` | bool | Recurse into subdirectories. Default `false`. |

## `create`

Idempotent `mkdir -p`.

Scalar-list form:

```yaml
create:
  - ~/.local/bin
  - ~/.config
```

Map form with per-entry mode:

```yaml
create:
  ~/.ssh:
    mode: 0o700
```

Mode literals accept octal (`0o700` or `0700`) and decimal.

## Conditionals

Conditional directives are not supported. Every directive in a profile
always runs. Split host- or OS-specific setup into separate profiles and
select them at apply time (e.g. `dfm apply base macos`).

## Path expansion

Applies to every path-valued field (`target`, `path`, `clean` entries,
`create` entries):

- Leading `~` → `$HOME`.
- `$VAR` / `${VAR}` → environment variable lookup (empty when unset).
- Relative paths in link `path` are resolved against the **base dir**
  (the profile file's directory, or `-C <dir>`). Absolute paths are
  used verbatim.

## Error handling

- **Parse errors** include the 1-based YAML line and column when
  available, prefixed with `line L:C:`.
- **Link errors** (per entry) are logged as warnings and the run
  continues — link directives never abort a profile.
- **Shell errors** increment `shell_failed` in the tally and continue.

## Worked example

```yaml
defaults:
  link:
    create: true
    relink: true

clean:
  - "~"
  - "~/.config"

create:
  - ~/.local/bin

link:
  ~/.zshrc: zshrc.zsh
  ~/.config/nvim:
    path: config/nvim
  ~/.hammerspoon:
    path: config/hammerspoon

shell:
  - name: Update submodules
    script: git submodule update --init --recursive
  - name: Install fonts
    script: ./scripts/setup-fonts.sh
    quiet: true
```
