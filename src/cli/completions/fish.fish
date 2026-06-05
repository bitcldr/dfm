# dfm fish completion
# Install: dfm completion fish > ~/.config/fish/completions/dfm.fish

# Disable file completion by default
complete -c dfm -f

# Global flags
complete -c dfm -s C -l dir     -r -d 'Base directory'      -F
complete -c dfm -s c -l config  -r -d 'Config path'         -F
complete -c dfm      -l verbose     -d 'Enable verbose (debug) logging'
complete -c dfm -s q -l quiet      -d 'Suppress progress output'
complete -c dfm      -l color   -r -d 'Colorize output (auto, always, never)' -a 'auto always never'

# Subcommands
complete -c dfm -n '__fish_use_subcommand' -a apply      -d 'Apply one or more profiles'
complete -c dfm -n '__fish_use_subcommand' -a diff       -d 'Show planned changes without writing'
complete -c dfm -n '__fish_use_subcommand' -a doctor     -d 'Verify installed symlinks still resolve'
complete -c dfm -n '__fish_use_subcommand' -a list       -d 'List available profiles'
complete -c dfm -n '__fish_use_subcommand' -a status     -d 'Show last applied profiles'
complete -c dfm -n '__fish_use_subcommand' -a completion -d 'Output shell completion script'

# Profile names for apply/diff (dynamic, calls dfm list)
complete -c dfm -n '__fish_seen_subcommand_from apply diff' \
    -a '(dfm -C $DFM_DIR list 2>/dev/null)' -d 'Profile'

# Shell names for completion subcommand
complete -c dfm -n '__fish_seen_subcommand_from completion' -a 'bash zsh fish'
