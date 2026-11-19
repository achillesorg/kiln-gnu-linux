#!/usr/bin/env bash
# F15 CI test: no network at idle outside the allow-list.
# Usage:
#   check-telemetry.sh static ROOT        scan package lists / rootfs for forbidden telemetry packages + NM connectivity
#   check-telemetry.sh pcap FILE.pcap     assert every DNS query / TLS SNI in a capture is in allowed-hosts.txt
#   check-telemetry.sh idle [SECS] [IF]   capture SECS (default 600) on IF (default any) with tcpdump, then run pcap check
# The VM job runs `idle` on the host bridge while the installed image sits idle for 10 minutes.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ALLOW="${KILN_ALLOWED_HOSTS:-$HERE/allowed-hosts.txt}"
POLICY="$HERE/../deb/kiln-base/usr/share/kiln/telemetry-policy.toml"
fail=0; bad() { echo "FAIL: $*" >&2; fail=1; }

static() {
  root="$1"
  pk=$(sed -n '/^\[forbidden_packages\]/,/^\[/p' "$POLICY" | grep -o '"[a-z0-9.+-]*"' | tr -d '"')
  for p in $pk; do
    hits=$(grep -rEl "^[[:space:]]*$p([[:space:]]|$)" "$root" --include='*.list*' 2>/dev/null || true)
    [ -z "$hits" ] || bad "$p listed in: $hits"
    [ -e "$root/var/lib/dpkg/info/$p.list" ] && bad "$p installed in rootfs"
  done
  conf="$root/etc/NetworkManager/conf.d/20-kiln-connectivity.conf"
  if [ -d "$root/etc/NetworkManager" ]; then
    grep -q '^enabled=false' "$conf" 2>/dev/null || bad "NM connectivity check not disabled ($conf)"
  fi
}

allowed() {  # $1 host -> 0 if in allow-list (suffix match)
  local h="${1,,}"
  while read -r a; do
    case "$a" in ''|'#'*) continue;; esac
    [ "$h" = "$a" ] || [[ "$h" == *".$a" ]] && return 0
  done < "$ALLOW"
  return 1
}

pcap() {
  command -v tshark >/dev/null || { echo "tshark required" >&2; exit 2; }
  hosts=$(tshark -r "$1" -Y 'dns.flags.response==0 || tls.handshake.extensions_server_name' \
    -T fields -e dns.qry.name -e tls.handshake.extensions_server_name 2>/dev/null | tr '\t,' '\n\n' | sort -u | sed '/^$/d')
  # non-DNS traffic to a bare IP without a prior DNS query is also suspicious
  for h in $hosts; do allowed "$h" || bad "contact to non-allowed host: $h"; done
}

idle() {
  secs="${1:-600}"; ifc="${2:-any}"; f="$(mktemp /tmp/kiln-idle.XXXXXX.pcap)"
  timeout "$secs" tcpdump -i "$ifc" -w "$f" -nn 'not arp and not (udp port 67 or 68) and not (ip6 and icmp6)' || true
  pcap "$f"
}

case "${1:-}" in
  static) static "${2:?root}";;
  pcap) pcap "${2:?file}";;
  idle) idle "${2:-}" "${3:-}";;
  selftest) allowed deb.debian.org && allowed x.security.debian.org && ! allowed evil.example.com && echo ok;;
  *) sed -n '2,8p' "$0"; exit 2;;
esac
exit $fail
