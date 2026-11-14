#!/usr/bin/env bash
#
# Glaze M4: copy the compositor-session test builds from ~/glaze-comp into
# ~/.local/glaze-m4/bin in the VM (the tty2 test session puts that first on
# PATH; the tty1 session's ~/.local/bin binaries are not touched).
#
# Usage: scripts/vm/m4-deploy.sh [debug|release]   (default: debug)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VM="${GLAZE_VM:-$ROOT/../vm}"
P="${1:-debug}"
"$VM" "set -e; mkdir -p ~/.local/glaze-m4/bin; cd ~/glaze-comp
  for b in target/$P/glaze-compositor target/$P/glaze-toplevel-ctl target/$P/glaze-session target/$P/glaze-service target/$P/glaze \
           apps/glaze-shell/target/$P/glaze-shell apps/glaze-settings/target/$P/glaze-settings; do
    [ -x \"\$b\" ] && install -m 755 \"\$b\" ~/.local/glaze-m4/bin/ && echo \"  \$b\"
  done; true"
echo "[vm] deployed to ~/.local/glaze-m4/bin"
