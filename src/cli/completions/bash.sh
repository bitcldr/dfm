# dfm bash completion
# Add to ~/.bashrc: source <(dfm completion bash)
_dfm_completion() {
    local cur prev words cword
    _init_completion || return

    local subcommands="apply diff doctor list status completion"

    # Global flag completions
    case "$prev" in
    -C|--dir)
        _filedir -d
        return
        ;;
    -c|--config)
        _filedir
        return
        ;;
    --color)
        COMPREPLY=($(compgen -W "auto always never" -- "$cur"))
        return
        ;;
    esac

    local global_flags="-C --dir -c --config --verbose -q --quiet --color"

    # No subcommand seen yet — offer subcommands and global flags.
    # Skip values of flags that consume the next word (-C, --dir, -c, --config, --color)
    # so a directory named "apply" passed to -C is not mistaken for the subcommand.
    local sub=""
    local i skip=0
    for (( i=1; i<cword; i++ )); do
        if (( skip )); then skip=0; continue; fi
        case "${words[i]}" in
        -C|--dir|-c|--config|--color) skip=1 ;;
        --verbose|-q|--quiet) ;;
        -*)  ;;
        *)
            if [[ " $subcommands " == *" ${words[i]} "* ]]; then
                sub="${words[i]}"
            fi
            break
            ;;
        esac
    done

    if [[ -z "$sub" ]]; then
        COMPREPLY=($(compgen -W "$subcommands $global_flags" -- "$cur"))
        return
    fi

    case "$sub" in
    apply|diff)
        local profiles
        profiles=$(dfm -C "${DFM_DIR:-.}" list 2>/dev/null)
        COMPREPLY=($(compgen -W "$profiles" -- "$cur"))
        ;;
    completion)
        COMPREPLY=($(compgen -W "bash zsh fish" -- "$cur"))
        ;;
    esac
}

complete -F _dfm_completion dfm
