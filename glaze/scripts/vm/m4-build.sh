#!/usr/bin/env bash
# Glaze M4: rsync the repo to ~/glaze-comp in the VM and build the session
# pieces: glaze-compositor + glaze-session (debug, or --release) and
# apps/glaze-shell (release). Then deploy them to ~/.local/glaze-m4/bin.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VM="${GLAZE_VM:-$ROOT/../vm}"
KEY="${GLAZE_VM_KEY:-$ROOT/../fedeos_key}"
P=debug; FLAG=""
[[ "${1:-}" == "--release" ]] && { P=release; FLAG=--release; }
SSH="ssh -i $KEY -p 2222 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o BatchMode=yes"
rsync -a --delete --exclude .git --exclude target --exclude 'apps/*/target' --exclude 'docs/screens' \
    -e "$SSH" "$ROOT/" fede@localhost:glaze-comp/
# Run detached in the VM (survives ssh drops), then wait for it.
"$VM" "rm -f ~/m4-build.log ~/m4-build.done; nohup sh -c '. ~/.cargo/env; cd ~/glaze-comp && flock /tmp/heavy-build.lock cargo build -j2 $FLAG -p glaze-compositor -p glaze-session; cd apps/glaze-shell && flock /tmp/heavy-build.lock cargo build -j2 --release; echo \$? > ~/m4-build.done' > ~/m4-build.log 2>&1 < /dev/null &"
until "$VM" -o ConnectTimeout=20 'test -f ~/m4-build.done' 2>/dev/null; do sleep 20; done
"$VM" "grep -E '^(error|warning)|^\s+-->|Finished' ~/m4-build.log | head -60"
"$ROOT/scripts/vm/m4-deploy.sh" "$P"
"$VM" "install -m 755 ~/glaze-comp/apps/glaze-shell/target/release/glaze-shell ~/.local/glaze-m4/bin/"
