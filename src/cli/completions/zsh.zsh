# dfm zsh completion
# Add to ~/.zshrc: source <(dfm completion zsh)
_dfm() {
    local state

    _arguments \
        '(-C --dir)'{-C,--dir}'[base directory]:dir:_files -/' \
        '(-c --config)'{-c,--config}'[config path]:file:_files' \
        '--verbose[enable verbose (debug) logging]' \
        '(-q --quiet)'{-q,--quiet}'[suppress progress output]' \
        '--color[colorize output]:color:(auto always never)' \
        '1: :->subcommand' \
        '*: :->args'

    case $state in
    subcommand)
        local subcommands
        subcommands=(
            'apply:apply one or more profiles'
            'diff:show planned changes without writing'
            'doctor:verify installed symlinks still resolve'
            'list:list available profiles'
            'status:show last applied profiles'
            'completion:output shell completion script'
        )
        _describe 'subcommand' subcommands
        ;;
    args)
        case ${words[2]} in
        apply|diff)
            local profiles
            profiles=(${(f)"$(dfm -C ${DFM_DIR:-.} list 2>/dev/null)"})
            _describe 'profile' profiles
            ;;
        completion)
            local shells; shells=('bash' 'zsh' 'fish')
            _describe 'shell' shells
            ;;
        esac
        ;;
    esac
}

compdef _dfm dfm
