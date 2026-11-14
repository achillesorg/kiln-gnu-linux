#!/usr/bin/env bash
# Build the Kiln GNU/Linux x86_64 live ISO.
#
#   build/x86/build-iso.sh [--slim] [--keep-work] [--no-cache]
#
# On the HOST (any Linux, WSL2, Git Bash with Docker Desktop): builds the Docker image `kiln-builder` from
# build/x86/Dockerfile, then runs this same script inside it with `docker run --privileged`.
# Inside the container (--in-container, done for you): stage 1 compiles Glaze and the Kiln apps, stage 2 runs live-build.
#
# Output: ./out/kiln-gnulinux-amd64[-slim].iso and ./out/<iso>.sha256   (./out is relative to the repo root).
# Environment: KILN_OUT (output dir), KILN_ISO_VARIANT=slim (same as --slim), KILN_DOCKER (docker command, default: docker)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SLIM=0; KEEP=0; NOCACHE=0; INSIDE=0
[ "${KILN_ISO_VARIANT:-}" = slim ] && SLIM=1
for a in "$@"; do
  case "$a" in
    --slim) SLIM=1 ;;
    --keep-work) KEEP=1 ;;
    --no-cache) NOCACHE=1 ;;
    --in-container) INSIDE=1 ;;
    -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "unknown argument: $a" >&2; exit 2 ;;
  esac
done
log() { printf '\n\033[1;34m[kiln-iso]\033[0m %s\n' "$*"; }

# --------------------------------------------------------------------------------------------- host side
if [ "$INSIDE" = 0 ]; then
  DOCKER="${KILN_DOCKER:-docker}"
  OUT="${KILN_OUT:-$ROOT/out}"
  command -v "$DOCKER" >/dev/null || { echo "docker not found (install Docker Desktop with the WSL2 backend, or docker.io on Linux)" >&2; exit 1; }
  mkdir -p "$OUT"
  log "building the builder image (linux/amd64); first run downloads about 1.5 GB and takes 10-20 minutes"
  bargs=(); [ "$NOCACHE" = 1 ] && bargs+=(--no-cache)
  "$DOCKER" build --platform linux/amd64 "${bargs[@]}" -t kiln-builder "$ROOT/build/x86"
  log "running the build (privileged: live-build needs to mount filesystems). Expect 40-120 minutes."
  # Source is mounted read-only and copied inside, so Windows/NTFS bind-mount quirks never touch the build.
  # Named volumes keep the work tree and the crate registry between runs (docker volume rm kiln-work kiln-cargo to reset).
  "$DOCKER" run --rm --privileged --platform linux/amd64 \
    -v "$ROOT":/src:ro \
    -v "$OUT":/out \
    -v kiln-work:/work \
    -v kiln-cargo-registry:/opt/cargo/registry -v kiln-cargo-git:/opt/cargo/git \
    -e KILN_ISO_VARIANT="$([ "$SLIM" = 1 ] && echo slim)" -e KILN_KEEP="$KEEP" \
    kiln-builder bash /src/build/x86/build-iso.sh --in-container
  log "done: $OUT"
  ls -l "$OUT" | sed 's/^/    /'
  exit 0
fi

# ---------------------------------------------------------------------------------------- container side
[ "$(id -u)" = 0 ] || { echo "--in-container must run as root" >&2; exit 1; }
[ "$(dpkg --print-architecture)" = amd64 ] || { echo "builder must be amd64 (got $(dpkg --print-architecture)); use --platform linux/amd64" >&2; exit 1; }
SRC=/src; WORK=/work/tree; OUT=/out
[ -d "$SRC/glaze" ] || { echo "$SRC/glaze missing: mount the repo at /src" >&2; exit 1; }
[ "${KILN_KEEP:-0}" = 1 ] && KEEP=1
mkdir -p "$OUT" /work

log "copying sources to $WORK"
mkdir -p "$WORK"
rsync -a --delete --exclude .git --exclude 'target' --exclude 'out' \
  "$SRC/" "$WORK/"
# keep cargo target dirs in the (volume) work tree between runs
mkdir -p /work/target-root /work/target-shell /work/target-settings

# ---- stage 1a: Glaze (Rust). Workspace crates share one target dir; the two GTK4 apps are their own workspaces.
log "stage 1a: Glaze (release)"
cd "$WORK/glaze"
export CARGO_TARGET_DIR=/work/target-root
cargo build --release -j"$(nproc)" -p glaze-cli -p glaze-service -p glaze-session -p glaze-compositor
for app in glaze-shell glaze-settings; do
  ( cd "apps/$app" && CARGO_TARGET_DIR="/work/target-${app#glaze-}" cargo build --release -j"$(nproc)" )
done
unset CARGO_TARGET_DIR
# stage.sh looks for <repo>/target/release and <repo>/apps/<app>/target/release: link the volume dirs there
mkdir -p "$WORK/glaze/target" "$WORK/glaze/apps/glaze-shell/target" "$WORK/glaze/apps/glaze-settings/target"
ln -sfn /work/target-root/release "$WORK/glaze/target/release"
ln -sfn /work/target-shell/release "$WORK/glaze/apps/glaze-shell/target/release"
ln -sfn /work/target-settings/release "$WORK/glaze/apps/glaze-settings/target/release"

# ---- stage 1b: Kiln apps (kiln-apps/build-all.sh logic; build deps are already in the image)
log "stage 1b: Kiln apps (Files, Terminal, Text, Calculator, Monitor, Viewer, Store)"
export KILN_SKIP_DEPS=1 BUILD=/work/kiln-apps-build JOBS="$(nproc)"
mkdir -p "$BUILD"
bash "$WORK/kiln-apps/build-all.sh"

# ---- stage 2: stage the tree into the live-build config and build the ISO
log "stage 2: live-build (amd64)"
IMG="$WORK/glaze/packaging/kilnlinux/image"
GLAZE_REPO="$WORK/glaze" GLAZE_PREFIX=/nonexistent KILN_LOCAL=/usr/local bash "$IMG/stage.sh"
export KILN_ARCH=amd64 KILN_ISO_WORK=/work/iso KILN_ISO_OUT="$OUT" KILN_NO_LOCK=1 KILN_MIN_FREE_GB=14
bargs=(); [ "${KILN_ISO_VARIANT:-}" = slim ] && bargs+=(--slim); [ "$KEEP" = 1 ] && bargs+=(--keep)
bash "$IMG/build.sh" "${bargs[@]}"
log "ISO written to $OUT"
