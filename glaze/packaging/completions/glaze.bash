# bash completion for the Glaze command-line client (glaze).
#
# Mirrors the real interface of crates/glaze-cli/src/args.rs:
#   apps | search <query> [--limit N] | launch <id> | exec-line <id> |
#   status | config get [key] | config set <key> <value> | ipc <request-json>
# plus -h/--help and -V/--version. Keep this in sync with the source and with
# docs/man/glaze.1.
#
# Install (system-wide):
#   sudo install -m 0644 glaze.bash \
#     /usr/share/bash-completion/completions/glaze
# Or source it directly in the current shell:
#   source glaze.bash

_glaze_config_keys() {
    COMPREPLY=($(compgen -W "appearance.theme appearance.scheme \
appearance.reduced_motion panel.position panel.height panel.clock_24h \
panel.show_seconds launcher.pinned" -- "${COMP_WORDS[COMP_CWORD]}"))
}

_glaze_config_values() {
    local key="${COMP_WORDS[COMP_CWORD-1]}"
    case "$key" in
        appearance.theme)   COMPREPLY=($(compgen -W "clay classic" -- "${COMP_WORDS[COMP_CWORD]}")) ;;
        appearance.scheme)  COMPREPLY=($(compgen -W "light dark" -- "${COMP_WORDS[COMP_CWORD]}")) ;;
        panel.position)     COMPREPLY=($(compgen -W "top bottom left right" -- "${COMP_WORDS[COMP_CWORD]}")) ;;
        appearance.reduced_motion|panel.clock_24h|panel.show_seconds)
            COMPREPLY=($(compgen -W "true false" -- "${COMP_WORDS[COMP_CWORD]}")) ;;
        *) : ;;
    esac
}

_glaze() {
    local cur prev cmd
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    if [ "$COMP_CWORD" -eq 1 ]; then
        COMPREPLY=($(compgen -W "apps search launch exec-line status config ipc \
-h --help -V --version" -- "$cur"))
        return
    fi

    cmd="${COMP_WORDS[1]}"

    case "$cmd" in
        search)
            case "$prev" in
                --limit) : ;;                       # numeric value; no list
                *) COMPREPLY=($(compgen -W "--limit" -- "$cur")) ;;
            esac
            ;;
        config)
            if [ "$COMP_CWORD" -eq 2 ]; then
                COMPREPLY=($(compgen -W "get set" -- "$cur"))
                return
            fi
            case "${COMP_WORDS[2]}" in
                get)
                    _glaze_config_keys
                    ;;
                set)
                    # Words: glaze config set <key> <value>
                    if [ "$COMP_CWORD" -eq 3 ]; then
                        _glaze_config_keys
                    else
                        _glaze_config_values
                    fi
                    ;;
            esac
            ;;
        launch|exec-line)
            # Application ids come from the live registry; leave them to the user.
            :
            ;;
        ipc)
            : # free-form JSON request
            ;;
        *)
            COMPREPLY=()
            ;;
    esac
}

complete -F _glaze glaze
