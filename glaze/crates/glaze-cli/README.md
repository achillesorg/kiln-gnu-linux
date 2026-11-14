# glaze-cli — the `glaze` command

`glaze` is the command-line client for the Glaze desktop. It talks to the same
portable core that the shell and settings app use: the **real** XDG application
registry (`glaze-service::registry`), the ranking search (`glaze-service::search`),
the shell-free launch path (`glaze-service::launch`) and the versioned
configuration (`glaze-common::config`).

It is deliberately **portable**: parsing and formatting are pure functions with
unit tests that run on any host, including the Windows authoring host. The only
Unix-specific feature is `glaze ipc`, which speaks to a running `glaze-service`
over the control socket; it is behind `#[cfg(unix)]`.

Build and run from the workspace root:

```sh
cargo run -p glaze-cli -- --help
```

## Commands

### `glaze apps`

List every application discovered from the real `.desktop` files in the XDG data
directories, as an aligned table (`id`, `name`, `terminal`).

```sh
$ glaze apps
id                     name              terminal
---------------------  ----------------  --------
glaze-settings          Glaze Settings     no
org.mozilla.firefox    Firefox           no
org.gnome.Terminal     Terminal          yes
```

### `glaze search <query> [--limit N]`

Ranked results using the documented `glaze-service` scoring function (exact >
prefix > word-prefix > substring > subsequence, plus keyword/id contributions).
`--limit` accepts both `--limit N` and `--limit=N`; the default is 20 and `0`
means "use the default".

```sh
$ glaze search web --limit 3
score  app id                name
-----  --------------------  ------------------
1000   firefox.desktop       Firefox
600    glaze-browser.desktop  Glaze Browser
300    org.gnome.Web.desktop Web
```

### `glaze launch <id>`

Looks the id up in the registry, prints the exact argv it is about to run, and
then launches it detached. `Exec` field codes are expanded and the argv is
**never** handed to a shell. Unknown ids fail cleanly.

```sh
$ glaze launch org.mozilla.firefox
argv: ["/usr/bin/firefox"]
launched Firefox (pid 4211)
```

### `glaze exec-line <id>`

Prints only the exact argv vector as JSON. This is the quick way to prove that a
`.desktop` `Exec` line is tokenised without a shell — shell metacharacters stay
inert argv elements.

```sh
$ glaze exec-line glaze-settings
[
  "/usr/bin/glaze-settings"
]
```

### `glaze status`

Prints the product version, protocol version, registry size, the resolved config
path and control-socket path, and (on Unix) whether the control socket currently
exists.

```sh
$ glaze status
Glaze Desktop 0.1.0
protocol      : 1
apps          : 137
theme         : clay (light)
config file   : /home/me/.config/glaze/config.toml
control socket: /run/user/1000/glaze/control.sock
socket exists : no
```

### `glaze config get [key]` / `glaze config set <key> <value>`

Read or persist the versioned Glaze configuration. `get` with no key prints the
whole `config.toml`; with a key it prints just that value. `set` validates the
value, writes the file (creating the directory as needed) and prints the
resulting config path. A missing file loads as defaults.

Supported keys:

| Key | Accepted values |
|---|---|
| `appearance.theme` | `clay`, `classic` |
| `appearance.scheme` | `light`, `dark` |
| `appearance.reduced_motion` | `true`/`false`/`yes`/`no`/`on`/`off`/`1`/`0` |
| `panel.position` | `top`, `bottom`, `left`, `right` |
| `panel.height` | integer logical px, `16`–`512` |
| `panel.clock_24h` | boolean (as above) |
| `panel.show_seconds` | boolean (as above) |
| `launcher.pinned` | comma/semicolon separated desktop ids; `""` clears |

```sh
$ glaze config set appearance.theme classic
appearance.theme = classic
wrote /home/me/.config/glaze/config.toml

$ glaze config get appearance.theme
classic
config: /home/me/.config/glaze/config.toml

$ glaze config set panel.height 4
glaze: invalid value "4" for panel.height: height must be between 16 and 512
```

### `glaze ipc <request-json>` (Unix only)

Sends a raw JSON `Request` from `glaze-common::ipc` over the control socket and
prints the pretty JSON `Response`. On non-Unix platforms it prints an honest
"unsupported" message instead of pretending.

```sh
$ glaze ipc '{"kind":"ping"}'
"pong"

$ glaze ipc '{"kind":"search-apps","query":"term","limit":5}'
{ "kind": "search-results", "hits": [ ... ] }
```

## Options

- `-h`, `--help` (also `glaze help`) — usage.
- `-V`, `--version` (also `glaze version`) — product name and version.
- No arguments prints help.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | success |
| `1` | user error (unknown command/flag, missing arg, bad value, unknown app id) |
| `2` | internal error (config I/O, IPC transport) |

## Tests

```sh
cargo test -p glaze-cli
```

`src/args.rs` tests every subcommand plus unknown flags, missing/extra
arguments and bad `config set` values; `src/output.rs` tests table alignment,
JSON output and `Response` rendering.
