"""Package backends for Kiln Store: apt (via apt-cache / dpkg-query, privileged steps through pkexec) and Flatpak (CLI)."""
import json
import os
import re
import shutil
import subprocess
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


def apt_cmd(action, pid):
    base = ["pkexec", "env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "-y"]
    if action == "install":
        return base + ["install", "--no-install-recommends", pid]
    return base + ["remove", pid]


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


def command_for(action, pkg):
    return apt_cmd(action, pkg.pid) if pkg.source == "apt" else flatpak_cmd(action, pkg)
