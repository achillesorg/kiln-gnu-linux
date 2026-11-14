# bash completion for the Glaze session launcher (glaze-session).
#
# Mirrors the real interface of crates/glaze-session/src/main.rs:
#   env|check|run|status  +  --backend <auto|sway|glaze|none>  and  --nested
# Keep this in sync with the source and with docs/man/glaze-session.1.
#
# Install (system-wide):
#   sudo install -m 0644 glaze-session.bash \
#     /usr/share/bash-completion/completions/glaze-session
# Or source it directly in the current shell:
#   source glaze-session.bash

_glaze_session_backends() {
    COMPREPLY=($(compgen -W "auto sway glaze none" -- "${COMP_WORDS[COMP_CWORD]}"))
}

_glaze_session() {
    local cur prev cmd
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    # First positional argument is the subcommand.
    if [ "$COMP_CWORD" -eq 1 ]; then
        COMPREPLY=($(compgen -W "env check run status -h --help" -- "$cur"))
        return
    fi

    cmd="${COMP_WORDS[1]}"

    # The --backend value is a fixed enum.
    if [ "$prev" = "--backend" ]; then
        _glaze_session_backends
        return
    fi

    case "$cmd" in
        env)
            COMPREPLY=($(compgen -W "--backend -h --help" -- "$cur"))
            ;;
        check)
            COMPREPLY=($(compgen -W "--backend -h --help" -- "$cur"))
            ;;
        run)
            COMPREPLY=($(compgen -W "--backend --nested -h --help" -- "$cur"))
            ;;
        status)
            COMPREPLY=($(compgen -W "-h --help" -- "$cur"))
            ;;
        *)
            COMPREPLY=($(compgen -W "env check run status -h --help" -- "$cur"))
            ;;
    esac
}

complete -F _glaze_session glaze-session
