"""Package backends for Kiln Store: apt (via apt-cache / dpkg-query, privileged steps through pkexec) and Flatpak (CLI)."""
import glob
import json
import os
import re
import shutil
import socket
import subprocess
import urllib.request
from dataclasses import dataclass


@dataclass
class Pkg:
    source: str          # "apt" or "flatpak"
    pid: str             # apt package name or flatpak application id
    name: str
    summary: str
    installed: bool = False
    version: str = ""
    origin: str = ""     # flatpak remote


def _run(cmd, timeout=60):
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, env={**os.environ, "LC_ALL": "C.UTF-8"})
        return r.returncode, r.stdout, r.stderr
    except (OSError, subprocess.TimeoutExpired) as e:
        return 1, "", str(e)


# ---------------------------------------------------------------- apt
def apt_installed_set():
    rc, out, _ = _run(["dpkg-query", "-W", "-f=${Package} ${db:Status-Abbrev}\n"])
    return {l.split()[0] for l in out.splitlines() if len(l.split()) >= 2 and l.split()[1].startswith("ii")}


def apt_search(query, limit=60):
    query = query.strip()
    if not query:
        return []
    rc, out, _ = _run(["apt-cache", "search", "--names-only", query]) if len(query) > 2 else _run(["apt-cache", "search", query])
    inst = apt_installed_set()
    res = []
    for line in out.splitlines():
        if " - " not in line:
            continue
        pid, summary = line.split(" - ", 1)
        # keep the list about applications: skip libraries, headers, debug symbols, docs
        if re.search(r"(^lib|-dev$|-dbg$|-doc$|-data$|-common$|-dbgsym$)", pid):
            continue
        res.append(Pkg("apt", pid, pid, summary, pid in inst))
    res.sort(key=lambda p: (query.lower() not in p.pid.lower(), not p.pid.lower().startswith(query.lower()), len(p.pid)))
    return res[:limit]


def apt_installed_apps(limit=400):
    """Installed packages that ship a desktop file (i.e. apps), via dpkg's file lists."""
    rc, out, _ = _run(["sh", "-c", "grep -l '/usr/share/applications/.*\\.desktop' /var/lib/dpkg/info/*.list 2>/dev/null"], 30)
    names = sorted({os.path.basename(l).removesuffix(".list").split(":")[0] for l in out.splitlines()})
    res = []
    for n in names[:limit]:
        rc, o, _ = _run(["dpkg-query", "-W", "-f=${binary:Summary}", n], 10)
        res.append(Pkg("apt", n, n, o.strip(), True))
    return res


def apt_details(pid):
    rc, out, _ = _run(["apt-cache", "show", pid])
    desc, ver = "", ""
    cur = None
    for line in out.splitlines():
        if line.startswith("Version:") and not ver:
            ver = line.split(":", 1)[1].strip()
        if line.startswith("Description"):
            cur = []
        elif cur is not None and line.startswith(" "):
            cur.append(line.strip().replace(" .", ""))
        elif cur is not None and line and not line.startswith(" "):
            break
    return ver, "\n".join(cur or [])


def priv_prefix():
    """Command prefix for privileged steps: pkexec (polkit), else passwordless sudo (live session), else None."""
    if shutil.which("pkexec"):
        return ["pkexec"]
    if shutil.which("sudo") and _run(["sudo", "-n", "true"], 5)[0] == 0:
        return ["sudo", "-n"]
    return None


def ensure_polkit_agent():
    """pkexec and Flatpak's polkit prompts need an agent in the session; Glaze does not autostart one, so start Kiln's (MATE) agent."""
    if _run(["pgrep", "-f", "polkit.*agent"], 5)[0] == 0:
        return True
    for pat in ("/usr/lib/*/mate-polkit/polkit-mate-authentication-agent-1", "/usr/lib/mate-polkit/polkit-mate-authentication-agent-1",
                "/usr/lib/policykit-1-gnome/polkit-gnome-authentication-agent-1", "/usr/lib/*/libexec/polkit-agent-helper-1-x"):
        for path in glob.glob(pat):
            try:
                subprocess.Popen([path], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
                return True
            except OSError:
                pass
    return False


APT_ENV = ["env", "DEBIAN_FRONTEND=noninteractive", "LC_ALL=C.UTF-8"]
NO_PRIV = "No way to get administrator rights: install the 'pkexec' package (sudo apt install pkexec)."


def apt_cmd(action, pid):
    pre = priv_prefix()
    if pre is None:
        return None
    base = pre + APT_ENV + ["apt-get", "-y"]
    if action == "install":
        return base + ["install", "--no-install-recommends", pid]
    return base + ["remove", pid]


def apt_update_cmd():
    pre = priv_prefix()
    return pre + APT_ENV + ["apt-get", "update"] if pre else None


def apt_upgrade_cmd(names):
    pre = priv_prefix()
    if not pre:
        return None
    return pre + APT_ENV + ["apt-get", "-y", "install", "--only-upgrade"] + list(names)


def parse_apt_upgradable(out):
    """`apt list --upgradable` lines: `name/suite new-ver arch [upgradable from: old-ver]`."""
    res = []
    for line in out.splitlines():
        m = re.match(r"^([^/\s]+)/\S+\s+(\S+)\s+\S+\s+\[upgradable from: ([^\]]+)\]", line)
        if m:
            res.append(Pkg("apt", m.group(1), m.group(1), f"{m.group(3)} to {m.group(2)}", True, m.group(2)))
    return res


def apt_upgradable():
    return parse_apt_upgradable(_run(["apt", "list", "--upgradable"], 60)[1])


def apt_lists_empty():
    return not glob.glob("/var/lib/apt/lists/*Packages*")


# ---------------------------------------------------------------- flatpak
def flatpak_available():
    return shutil.which("flatpak") is not None


def flatpak_installed_ids():
    rc, out, _ = _run(["flatpak", "list", "--app", "--columns=application"])
    return {l.strip() for l in out.splitlines() if l.strip()}


def flatpak_search(query):
    if not flatpak_available() or not query.strip():
        return []
    rc, out, _ = _run(["flatpak", "search", "--columns=application,name,description,version,remotes", query], 60)
    inst = flatpak_installed_ids()
    res = []
    for line in out.splitlines():
        parts = line.split("\t")
        if len(parts) < 3 or "." not in parts[0]:
            continue
        app, name, desc = parts[0].strip(), parts[1].strip(), parts[2].strip()
        ver = parts[3].strip() if len(parts) > 3 else ""
        remote = parts[4].strip().split(",")[0] if len(parts) > 4 else "flathub"
        res.append(Pkg("flatpak", app, name, desc, app in inst, ver, remote))
    return res


def flatpak_installed_apps():
    if not flatpak_available():
        return []
    rc, out, _ = _run(["flatpak", "list", "--app", "--columns=application,name,description,version,origin"])
    res = []
    for line in out.splitlines():
        p = line.split("\t")
        if len(p) >= 2:
            res.append(Pkg("flatpak", p[0], p[1], p[2] if len(p) > 2 else "", True, p[3] if len(p) > 3 else "", p[4] if len(p) > 4 else ""))
    return res


def flatpak_cmd(action, pkg):
    if action == "install":
        return ["flatpak", "install", "-y", "--noninteractive", "--system", pkg.origin or "flathub", pkg.pid]
    return ["flatpak", "uninstall", "-y", "--noninteractive", pkg.pid]


def flatpak_ensure_remote():
    """Make sure the Flathub remote exists (normally provisioned by the image)."""
    rc, out, _ = _run(["flatpak", "remotes", "--columns=name"])
    if "flathub" in out.split():
        return True   # system-wide Flathub remote is provisioned by Kiln; installs go through flatpak's own polkit prompt
    rc, _, _ = _run(["flatpak", "remote-add", "--system", "--if-not-exists", "flathub", "https://dl.flathub.org/repo/flathub.flatpakrepo"], 120)
    return rc == 0


def flatpak_have_appstream():
    return bool(glob.glob("/var/lib/flatpak/appstream/*/*/active/appstream.xml*")) or \
        bool(glob.glob(os.path.expanduser("~/.local/share/flatpak/appstream/*/*/active/appstream.xml*")))


def flatpak_refresh_appstream():
    """Empty Flathub listing root cause: remote added but appstream never pulled, so `flatpak search` prints nothing."""
    if not flatpak_available() or not flatpak_ensure_remote():
        return False
    return _run(["flatpak", "update", "--appstream", "-y", "--noninteractive"], 300)[0] == 0


def parse_flatpak_updates(out):
    res = []
    for line in out.splitlines():
        p = [x.strip() for x in line.split("\t")]
        if p and "." in p[0]:
            res.append(Pkg("flatpak", p[0], p[1] if len(p) > 1 else p[0], "Update available", True, p[2] if len(p) > 2 else "", p[3] if len(p) > 3 else "flathub"))
    return res


def flatpak_updates():
    if not flatpak_available():
        return []
    return parse_flatpak_updates(_run(["flatpak", "remote-ls", "--updates", "--app", "--columns=application,name,version,origin"], 120)[1])


def flatpak_update_cmd():
    return ["flatpak", "update", "-y", "--noninteractive"]


def parse_flatpak_info(out):
    """`flatpak remote-info` / `info`: header lines `Key: value` after a title line."""
    d = {}
    for line in out.splitlines():
        if ":" in line:
            k, v = line.split(":", 1)
            d[k.strip().lower()] = v.strip()
    return d


def flatpak_details(pkg):
    cmd = ["flatpak", "info", pkg.pid] if pkg.installed else ["flatpak", "remote-info", pkg.origin or "flathub", pkg.pid]
    rc, out, _ = _run(cmd, 30)
    d = parse_flatpak_info(out)
    first = out.strip().splitlines()[0] if out.strip() else ""
    return {"version": d.get("version", ""), "size": d.get("download", d.get("installed", "")),
            "license": d.get("license", ""), "text": first.split(" - ", 1)[1] if " - " in first else pkg.summary}


# ---------------------------------------------------------------- network, curated data, categories
def online(host="flathub.org", timeout=2.5):
    try:
        socket.create_connection((host, 443), timeout).close()
        return True
    except OSError:
        return False


def flathub_api_search(query, limit=40):
    """Fallback when the local appstream is missing: Flathub's public search API (needs network)."""
    try:
        req = urllib.request.Request("https://flathub.org/api/v2/search", data=json.dumps({"query": query}).encode(),
                                     headers={"Content-Type": "application/json", "User-Agent": "kiln-store"})
        data = json.load(urllib.request.urlopen(req, timeout=8))
    except (OSError, ValueError):
        return []
    inst = flatpak_installed_ids() if flatpak_available() else set()
    return [Pkg("flatpak", h["app_id"], h.get("name", h["app_id"]), h.get("summary", ""), h["app_id"] in inst, "", "flathub")
            for h in data.get("hits", [])[:limit] if h.get("app_id")]


# Curated, offline-safe catalogue: (source, id, name, summary). Shown when the network or package lists are unavailable.
CURATED = {
    "Internet": [("flatpak", "org.mozilla.firefox", "Firefox", "Web browser"), ("flatpak", "org.mozilla.Thunderbird", "Thunderbird", "Email client"),
                 ("flatpak", "org.chromium.Chromium", "Chromium", "Web browser"), ("apt", "transmission-gtk", "Transmission", "BitTorrent client"),
                 ("flatpak", "com.discordapp.Discord", "Discord", "Chat and voice")],
    "Office": [("apt", "libreoffice", "LibreOffice", "Office suite"), ("apt", "evince", "Document Viewer", "PDF and document viewer"),
               ("apt", "keepassxc", "KeePassXC", "Password manager"), ("flatpak", "md.obsidian.Obsidian", "Obsidian", "Notes")],
    "Media": [("apt", "vlc", "VLC", "Media player"), ("apt", "mpv", "mpv", "Minimal media player"), ("apt", "audacity", "Audacity", "Audio editor"),
              ("apt", "kdenlive", "Kdenlive", "Video editor"), ("flatpak", "com.spotify.Client", "Spotify", "Music streaming")],
    "Graphics": [("apt", "gimp", "GIMP", "Image editor"), ("apt", "inkscape", "Inkscape", "Vector graphics"), ("apt", "krita", "Krita", "Digital painting"),
                 ("apt", "blender", "Blender", "3D creation suite"), ("apt", "scribus", "Scribus", "Desktop publishing")],
    "Games": [("flatpak", "com.valvesoftware.Steam", "Steam", "Game store and launcher"), ("apt", "supertuxkart", "SuperTuxKart", "Kart racing"),
              ("flatpak", "net.lutris.Lutris", "Lutris", "Game manager"), ("flatpak", "com.usebottles.bottles", "Bottles", "Run Windows software")],
    "Develop": [("flatpak", "com.visualstudio.code", "Visual Studio Code", "Code editor"), ("apt", "git", "Git", "Version control"),
                ("apt", "geany", "Geany", "Lightweight IDE"), ("apt", "podman", "Podman", "Containers")],
    "System": [("apt", "gparted", "GParted", "Partition editor"), ("apt", "htop", "htop", "Process viewer"), ("apt", "obs-studio", "OBS Studio", "Screen recording and streaming"),
               ("flatpak", "com.github.tchx84.Flatseal", "Flatseal", "Flatpak permissions")],
}
CATEGORIES = ["Featured"] + list(CURATED)


def category_items(cat):
    if cat == "Featured":
        pool = [x for c in CURATED.values() for x in c[:2]]
    else:
        pool = CURATED.get(cat, [])
    apt_in, fp_in = apt_installed_set(), (flatpak_installed_ids() if flatpak_available() else set())
    res = []
    for src, pid, name, summ in pool:
        if src == "flatpak" and not flatpak_available():
            continue
        res.append(Pkg(src, pid, name, summ, pid in (apt_in if src == "apt" else fp_in), "", "flathub" if src == "flatpak" else ""))
    return res


def curated_search(query):
    q = query.lower()
    return [p for p in category_items_all() if q in p.pid.lower() or q in p.name.lower() or q in p.summary.lower()]


def category_items_all():
    seen, out = set(), []
    for c in CURATED:
        for p in category_items(c):
            if (p.source, p.pid) not in seen:
                seen.add((p.source, p.pid)); out.append(p)
    return out


def command_for(action, pkg):
    return apt_cmd(action, pkg.pid) if pkg.source == "apt" else flatpak_cmd(action, pkg)
