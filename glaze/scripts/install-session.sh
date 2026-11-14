#!/usr/bin/env bash
#
# Glaze — install (or remove) the Glaze Wayland session for the CURRENT USER.
#
# This is the no-root companion to `packaging/fedora/glaze.spec` (`GLAZE.md` §2:
# "Session installation is later an explicit opt-in using an RPM or documented
# local install"). Everything goes under $PREFIX (default `~/.local`), which is a
# standard XDG data location the display manager already scans for
# `wayland-sessions/`.
#
# What registering the session does NOT do:
#   * it never edits GDM/SDDM/LightDM configuration,
#   * it never changes your default session,
#   * it never touches the bootloader.
# It does write one file outside $PREFIX: the Glaze sway config at
# ${XDG_CONFIG_HOME:-~/.config}/glaze/sway.conf (also recorded in the manifest).
# Glaze simply appears as one more choice in the login screen's session menu, and
# `--uninstall` removes exactly the files this script wrote (recorded in a
# manifest) and nothing else.
#
# Usage:
#   scripts/install-session.sh [--release] [--prefix DIR] [--uninstall] [--help]

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="debug"
PREFIX="${HOME}/.local"
UNINSTALL=0

MANIFEST_REL="share/glaze/install-manifest.txt"

usage() {
    cat <<'EOF'
install-session.sh — install the Glaze Wayland session for the current user

USAGE:
    scripts/install-session.sh [OPTIONS]

OPTIONS:
    --release       Use the release build instead of the debug build.
    --prefix DIR    Install root (default: ~/.local).
    --uninstall     Remove the files a previous run installed.
    -h, --help      Show this help.

NOTES:
    * No root required; nothing outside DIR is modified.
    * Build first: scripts/build.sh [--release]
    * After installing, LOG OUT and pick the "Glaze" session at the login screen.
EOF
}

log()  { printf '\033[1;34m[glaze]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[glaze]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[glaze]\033[0m %s\n' "$*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --release)   PROFILE="release" ;;
        --prefix)    PREFIX="${2:?--prefix needs a directory}"; shift ;;
        --uninstall) UNINSTALL=1 ;;
        -h|--help)   usage; exit 0 ;;
        *)           die "unknown argument: $1 (try --help)" ;;
    esac
    shift
done

PREFIX="${PREFIX%/}"
MANIFEST="${PREFIX}/${MANIFEST_REL}"
BINDIR="${PREFIX}/bin"
DATADIR="${PREFIX}/share"

# --- uninstall ---------------------------------------------------------------

if [[ "$UNINSTALL" -eq 1 ]]; then
    [[ -f "$MANIFEST" ]] || die "no manifest at $MANIFEST — refusing to delete files this script did not install"
    removed=0
    while IFS= read -r file; do
        [[ -z "$file" || "$file" == \#* ]] && continue
        if [[ -e "$file" ]]; then
            rm -f -- "$file"
            removed=$((removed + 1))
        fi
    done <"$MANIFEST"
    rm -f -- "$MANIFEST"
    # Remove directories this script created, deepest first; only if empty.
    while IFS= read -r dir; do
        [[ -z "$dir" ]] && continue
        rmdir --ignore-fail-on-non-empty -- "$dir" 2>/dev/null || true
    done <<EOF
${DATADIR}/wayland-sessions
${DATADIR}/glaze/themes
${DATADIR}/glaze/assets
${DATADIR}/glaze
${DATADIR}/applications
${DATADIR}/man/man1
${BINDIR}
EOF
    log "uninstalled $removed file(s) from $PREFIX"
    log "log out to leave any running Glaze session"
    exit 0
fi

# --- install -----------------------------------------------------------------

command -v install >/dev/null 2>&1 || die "'install' not found (coreutils)"

BINARIES=(glaze-session glaze-service glaze-compositor glaze glaze-shell glaze-settings)
missing=()
for name in "${BINARIES[@]}"; do
    case "$name" in
        glaze-shell|glaze-settings)
            path="$ROOT/apps/$name/target/$PROFILE/$name" ;;
        *)
            path="$ROOT/target/$PROFILE/$name" ;;
    esac
    # The compositor is built by its own agent (often in ~/glaze-comp); it is
    # optional for the Sway session, so a missing one is skipped, not fatal.
    if [[ "$name" == glaze-compositor && ! -x "$path" ]]; then
        for alt in "$ROOT/../glaze-comp/target/$PROFILE/$name" "$ROOT/../glaze-comp/target/release/$name"; do
            [[ -x "$alt" ]] && { COMPOSITOR_BIN="$alt"; break; }
        done
        [[ -n "${COMPOSITOR_BIN:-}" ]] || warn "glaze-compositor not built; skipped (Sway session only)"
        continue
    fi
    [[ -x "$path" ]] || missing+=("$path")
done

if [[ "${#missing[@]}" -gt 0 ]]; then
    warn "these binaries are missing:"
    for path in "${missing[@]}"; do warn "  $path"; done
    die "build first: scripts/build.sh $([[ "$PROFILE" == release ]] && echo --release)"
fi

log "installing into $PREFIX ($PROFILE build)"
mkdir -p -- "$(dirname -- "$MANIFEST")"
: >"$MANIFEST"

track() { printf '%s\n' "$1" >>"$MANIFEST"; }

install_dir() {
    local dir="$1"
    mkdir -p -- "$dir"
}

install_file() {
    local src="$1" dst="$2" mode="$3"
    install_dir "$(dirname -- "$dst")"
    install -m "$mode" -- "$src" "$dst"
    track "$dst"
}

write_file() {
    local dst="$1" mode="$2"
    install_dir "$(dirname -- "$dst")"
    cat >"$dst"
    chmod "$mode" -- "$dst"
    track "$dst"
}

# Binaries.
for name in "${BINARIES[@]}"; do
    case "$name" in
        glaze-shell|glaze-settings)
            install_file "$ROOT/apps/$name/target/$PROFILE/$name" "$BINDIR/$name" 0755 ;;
        *)
            if [[ "$name" == glaze-compositor && ! -x "$ROOT/target/$PROFILE/$name" ]]; then
                [[ -n "${COMPOSITOR_BIN:-}" ]] && install_file "$COMPOSITOR_BIN" "$BINDIR/$name" 0755
            else
                install_file "$ROOT/target/$PROFILE/$name" "$BINDIR/$name" 0755
            fi ;;
    esac
done

# Bundled wallpapers (found via $XDG_DATA_HOME/glaze/wallpapers).
# Recursive: v2 sets live in subfolders (kiln/, draft/, ...). Sources (src/)
# and generator tools are not needed at runtime.
while IFS= read -r -d '' f; do
    rel="${f#"$ROOT"/assets/wallpapers/}"
    install_file "$f" "${DATADIR}/glaze/wallpapers/$rel" 0644
done < <(find "$ROOT/assets/wallpapers" -type f \( -name '*.png' -o -name '*.svg' -o -name '*.toml' -o -name '*.md' \) \
             -not -path '*/src/*' -not -path '*/tools/*' -print0)

# Glaze-owned sway config, passed to sway with `-c` by `glaze-session run
# --backend sway` (no bar, no swaybg, floating by default). It lives under the
# user's config dir, not $PREFIX, because glaze-session looks it up there.
SWAY_CONF_DST="${XDG_CONFIG_HOME:-$HOME/.config}/glaze/sway.conf"
install_file "$ROOT/packaging/sway/sway.conf" "$SWAY_CONF_DST" 0644

# Session entry. The Exec is an ABSOLUTE path so the session works even when
# ~/.local/bin is not on the display manager's PATH.
write_file "${DATADIR}/wayland-sessions/glaze.desktop" 0644 <<EOF
[Desktop Entry]
Type=Application
Name=Glaze
Comment=Glaze Desktop — opt-in native Wayland session (user install)
Exec=${BINDIR}/glaze-session run
TryExec=${BINDIR}/glaze-session
DesktopNames=Glaze
Keywords=desktop;session;wayland;glaze;
EOF

# Settings app entry, so it can be launched from any app menu.
write_file "${DATADIR}/applications/glaze-settings.desktop" 0644 <<EOF
[Desktop Entry]
Type=Application
Name=Glaze Settings
Comment=Configure Glaze Desktop
Exec=${BINDIR}/glaze-settings
TryExec=${BINDIR}/glaze-settings
Icon=glaze-mark
Terminal=false
Categories=Settings;DesktopSettings;
Keywords=glaze;settings;appearance;panel;
EOF

# Themes and assets (original/licensed only).
if [[ -d "$ROOT/themes" ]]; then
    install_dir "${DATADIR}/glaze/themes"
    cp -a -- "$ROOT"/themes/. "${DATADIR}/glaze/themes/"
    while IFS= read -r f; do track "$f"; done < <(find "${DATADIR}/glaze/themes" -type f)
fi
if [[ -d "$ROOT/assets" ]]; then
    install_dir "${DATADIR}/glaze/assets"
    cp -a -- "$ROOT"/assets/. "${DATADIR}/glaze/assets/"
    while IFS= read -r f; do track "$f"; done < <(find "${DATADIR}/glaze/assets" -type f)
fi

# Man pages and shell completions, when present.
if [[ -d "$ROOT/docs/man" ]]; then
    for page in "$ROOT"/docs/man/*.1; do
        [[ -e "$page" ]] || continue
        install_file "$page" "${DATADIR}/man/man1/$(basename -- "$page")" 0644
    done
fi
if [[ -d "$ROOT/packaging/completions" ]]; then
    for completion in "$ROOT"/packaging/completions/*.bash; do
        [[ -e "$completion" ]] || continue
        install_file "$completion" "${DATADIR}/bash-completion/completions/$(basename -- "$completion" .bash)" 0644
    done
    for completion in "$ROOT"/packaging/completions/*.zsh; do
        [[ -e "$completion" ]] || continue
        install_file "$completion" "${DATADIR}/zsh/site-functions/_$(basename -- "$completion" .zsh)" 0644
    done
fi

# A small user-facing note next to the install.
write_file "${DATADIR}/glaze/README.user-install.txt" 0644 <<EOF
Glaze was installed for this user only, under ${PREFIX}.

To use it:
  1. Log out.
  2. At the login screen choose the "Glaze" session.
Your existing desktop session is unchanged and still available.

To remove it:
  scripts/install-session.sh --uninstall --prefix ${PREFIX}
EOF

log "installed $(($(wc -l <"$MANIFEST"))) file(s)"
log "session entry: ${DATADIR}/wayland-sessions/glaze.desktop"
log "log out and pick \"Glaze\" at the login screen"
log "to undo: scripts/install-session.sh --uninstall --prefix ${PREFIX}"
