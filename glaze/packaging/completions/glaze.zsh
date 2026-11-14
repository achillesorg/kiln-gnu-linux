#compdef glaze
#
# zsh completion for the Glaze command-line client (glaze).
#
# Mirrors the real interface of crates/glaze-cli/src/args.rs:
#   apps | search <query> [--limit N] | launch <id> | exec-line <id> |
#   status | config get [key] | config set <key> <value> | ipc <request-json>
# plus -h/--help and -V/--version. Keep this in sync with the source and with
# docs/man/glaze.1.
#
# Install: copy to a directory on $fpath as _glaze, e.g.
#   cp glaze.zsh "${fpath[1]}/_glaze"
#   compinit

_glaze() {
    local -a config_keys
    config_keys=(
        'appearance.theme:clay | classic'
        'appearance.scheme:light | dark'
        'appearance.reduced_motion:a boolean'
        'panel.position:top | bottom | left | right'
        'panel.height:logical px, 16-512'
        'panel.clock_24h:a boolean'
        'panel.show_seconds:a boolean'
        'launcher.pinned:comma-separated desktop ids'
    )

    _arguments -C \
        '(-h --help)'{-h,--help}'[show usage]' \
        '(-V --version)'{-V,--version}'[show the version]' \
        '1:command:->cmd' \
        '*::argument:->args'

    case $state in
        cmd)
            local -a cmds
            cmds=(
                'apps:list discovered applications'
                'search:ranked application search'
                'launch:build argv and launch an application'
                'exec-line:print the exact launch argv as JSON'
                'status:show version, registry size and paths'
                'config:get or set configuration values'
                'ipc:send a raw IPC request (Unix only)'
            )
            _describe -t commands 'glaze command' cmds
            ;;
        args)
            case $words[1] in
                search)
                    _arguments '--limit=[maximum number of results]:limit:'
                    ;;
                config)
                    if (( CURRENT == 3 )); then
                        _values 'config subcommand' get set
                    else
                        case $words[2] in
                            get)
                                _describe -t keys 'config key' config_keys
                                ;;
                            set)
                                if (( CURRENT == 4 )); then
                                    _describe -t keys 'config key' config_keys
                                else
                                    case $words[3] in
                                        appearance.theme)  _values 'theme' clay classic ;;
                                        appearance.scheme) _values 'scheme' light dark ;;
                                        appearance.reduced_motion|panel.clock_24h|panel.show_seconds)
                                            _values 'boolean' true false ;;
                                        panel.position)    _values 'position' top bottom left right ;;
                                    esac
                                fi
                                ;;
                        esac
                    fi
                    ;;
                launch|exec-line|ipc)
                    : # ids / free-form JSON: no completion
                    ;;
            esac
            ;;
    esac
}

_glaze "$@"
