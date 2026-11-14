#!/usr/bin/env bash
# Switch the VM's active VT (the `kbd` package's chvt is not installed):
# VT_ACTIVATE + VT_WAITACTIVE on /dev/tty0 as root. Usage: scripts/vm/chvt.sh N
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VM="${GLAZE_VM:-$ROOT/../vm}"
N="${1:?usage: chvt.sh N}"
"$VM" "sudo timeout 10 python3 -c 'import fcntl,os,sys; n=int(sys.argv[1]); fd=os.open(\"/dev/tty0\", os.O_RDWR); fcntl.ioctl(fd, 0x5606, n); fcntl.ioctl(fd, 0x5607, n)' $N; cat /sys/class/tty/tty0/active"
