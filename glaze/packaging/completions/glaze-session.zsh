#compdef glaze-session
#
# zsh completion for the Glaze session launcher (glaze-session).
#
# Mirrors the real interface of crates/glaze-session/src/main.rs:
#   env|check|run|status  +  --backend <auto|sway|glaze|none>  and  --nested
# Keep this in sync with the source and with docs/man/glaze-session.1.
#
# Install: copy to a directory on $fpath as _glaze-session, e.g.
#   cp glaze-session.zsh "${fpath[1]}/_glaze-session"
#   compinit

_glaze_session() {
    local -a subcmds
    subcmds=(
        'env:print the session environment exports'
        'check:verify the components a session needs are present'
        'run:apply the environment and supervise the component graph'
        'status:print a JSON status snapshot'
    )

    _arguments -C \
        '(-h --help)'{-h,--help}'[show usage]' \
        '1:subcommand:->cmd' \
        '*::argument:->args'

    case $state in
        cmd)
            _describe -t commands 'glaze-session subcommand' subcmds
            ;;
        args)
            case $words[1] in
                env|check)
                    _arguments \
                        '--backend[compositor backend to plan for]:backend:(auto sway glaze none)' \
                        '(-h --help)'{-h,--help}'[show usage]'
                    ;;
                run)
                    _arguments \
                        '--backend[compositor backend to plan for]:backend:(auto sway glaze none)' \
                        '--nested[run the compositor nested inside the current session]' \
                        '(-h --help)'{-h,--help}'[show usage]'
                    ;;
                status)
                    _arguments \
                        '(-h --help)'{-h,--help}'[show usage]'
                    ;;
            esac
            ;;
    esac
}

_glaze_session "$@"
