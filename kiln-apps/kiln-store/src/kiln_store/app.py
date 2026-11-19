"""Kiln Store: browse, search, install and remove apps from apt and Flatpak/Flathub."""
import os
import subprocess
import sys
import threading

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gio, GLib, Gtk  # noqa: E402

from . import APP_ID, VERSION, backends  # noqa: E402

DATADIR = os.environ.get("KILN_THEME_DIR", "@themedir@")
FEATURED = [  # legacy list, superseded by backends.CURATED
    "firefox", "thunderbird", "libreoffice", "vlc", "gimp", "inkscape", "krita", "blender", "audacity", "obs-studio",
            "keepassxc", "transmission-gtk", "evince", "mpv", "kdenlive", "scribus"]


def load_theme():
    display = __import__("gi.repository.Gdk", fromlist=["Gdk"]).Display.get_default()
    provider = Gtk.CssProvider()
    sm = Adw.StyleManager.get_default()
    force = os.environ.get("KILN_SCHEME")  # test override, like the other Kiln apps
    if force in ("dark", "light"):
        sm.set_color_scheme(Adw.ColorScheme.FORCE_DARK if force == "dark" else Adw.ColorScheme.FORCE_LIGHT)

    def apply(*_):
        path = os.path.join(DATADIR, "kiln-dark.css" if sm.get_dark() else "kiln-light.css")
        if os.path.exists(path):
            provider.load_from_path(path)
    Gtk.StyleContext.add_provider_for_display(display, provider, Gtk.STYLE_PROVIDER_PRIORITY_USER)
    sm.connect("notify::dark", apply)
    apply()
    Gtk.Settings.get_default().set_property("gtk-icon-theme-name", "Kiln")


def combined_search(q, sel):
    """Debian + Flathub results; Flathub falls back to the web API, then to the curated list (offline)."""
    res = []
    if sel in (0, 1):
        res += backends.apt_search(q)
    if sel in (0, 2):
        fp = backends.flatpak_search(q)
        if not fp and backends.flatpak_available():
            fp = backends.flathub_api_search(q)
        res += fp
    if not res:
        res = [p for p in backends.curated_search(q) if sel == 0 or (sel == 1) == (p.source == "apt")]
    return res


class Row(Gtk.Box):
    def __init__(self, pkg, on_action, on_open):
        super().__init__(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        self.pkg, self.on_action = pkg, on_action
        self.add_css_class("card")
        self.set_margin_start(2); self.set_margin_end(2)
        text = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, hexpand=True)
        text.set_margin_top(10); text.set_margin_bottom(10); text.set_margin_start(14)
        head = Gtk.Box(spacing=8)
        title = Gtk.Label(label=pkg.name, xalign=0); title.add_css_class("heading")
        pill = Gtk.Label(label="Flathub" if pkg.source == "flatpak" else "Debian"); pill.add_css_class("caption"); pill.add_css_class("kiln-pill")
        head.append(title); head.append(pill)
        if pkg.version:
            v = Gtk.Label(label=pkg.version, xalign=0); v.add_css_class("dim-label"); v.add_css_class("caption"); head.append(v)
        summ = Gtk.Label(label=pkg.summary, xalign=0, wrap=True, lines=2, ellipsize=3)
        summ.add_css_class("dim-label")
        text.append(head); text.append(summ)
        click = Gtk.GestureClick()
        click.connect("released", lambda *_: on_open(pkg))
        text.add_controller(click)
        self.append(text)
        self.spinner = Gtk.Spinner()
        self.button = Gtk.Button(valign=Gtk.Align.CENTER)
        self.button.set_margin_end(14)
        self.button.connect("clicked", self.clicked)
        self.append(self.spinner); self.append(self.button)
        self.refresh()

    def refresh(self):
        self.button.set_label("Remove" if self.pkg.installed else "Install")
        for c in ("suggested-action", "destructive-action"):
            self.button.remove_css_class(c)
        self.button.add_css_class("destructive-action" if self.pkg.installed else "suggested-action")

    def busy(self, on):
        self.button.set_sensitive(not on)
        self.spinner.set_spinning(on)

    def clicked(self, _b):
        self.on_action(self, "remove" if self.pkg.installed else "install")


class Window(Adw.ApplicationWindow):
    def __init__(self, app):
        super().__init__(application=app, title="Kiln Store", default_width=860, default_height=680)
        self.view_name = "browse"       # browse | installed | updates
        self.category = "Featured"
        self.gen = 0                     # generation counter: drop results of superseded async jobs
        self.timeout = 0
        self.toasts = Adw.ToastOverlay()
        tv = Adw.ToolbarView()
        header = Adw.HeaderBar()
        self.search = Gtk.SearchEntry(placeholder_text="Search apps (Debian and Flathub)", hexpand=True, width_chars=36)
        self.search.connect("activate", self.do_search)
        self.search.connect("search-changed", self.search_changed)
        header.set_title_widget(self.search)
        self.mode = Gtk.DropDown.new_from_strings(["All sources", "Debian (apt)", "Flathub"])
        self.mode.connect("notify::selected", lambda *_: self.reload())
        header.pack_end(self.mode)
        self.tabs = {}
        box = Gtk.Box(css_classes=["linked"])
        group = None
        for key, label in (("browse", "Browse"), ("installed", "Installed"), ("updates", "Updates")):
            b = Gtk.ToggleButton(label=label)
            if group is None:
                group = b; b.set_active(True)
            else:
                b.set_group(group)
            b.connect("toggled", self.tab_toggled, key)
            box.append(b); self.tabs[key] = b
        header.pack_start(box)
        tv.add_top_bar(header)
        # category chips
        self.cats = Gtk.Box(spacing=6, css_classes=["linked"])
        cgroup = None
        for c in backends.CATEGORIES:
            b = Gtk.ToggleButton(label=c)
            if cgroup is None:
                cgroup = b; b.set_active(True)
            else:
                b.set_group(cgroup)
            b.connect("toggled", self.cat_toggled, c)
            self.cats.append(b)
        self.cat_scroll = Gtk.ScrolledWindow(vscrollbar_policy=Gtk.PolicyType.NEVER, child=self.cats)
        self.cat_scroll.set_margin_start(16); self.cat_scroll.set_margin_end(16); self.cat_scroll.set_margin_top(8)
        tv.add_top_bar(self.cat_scroll)
        self.banner = Adw.Banner(revealed=False)
        tv.add_top_bar(self.banner)
        self.update_all = Gtk.Button(label="Update all", halign=Gtk.Align.END, visible=False, css_classes=["suggested-action"])
        self.update_all.set_margin_end(16); self.update_all.set_margin_top(8)
        self.update_all.connect("clicked", self.do_update_all)
        tv.add_top_bar(self.update_all)
        self.stack = Adw.ViewStack()
        self.list = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
        self.list.add_css_class("kiln-plain")
        scroll = Gtk.ScrolledWindow(vexpand=True)
        clamp = Adw.Clamp(maximum_size=760)
        clamp.set_margin_top(12); clamp.set_margin_bottom(12); clamp.set_margin_start(16); clamp.set_margin_end(16)
        clamp.set_child(self.list); scroll.set_child(clamp)
        self.status = Adw.StatusPage(icon_name="system-software-install-symbolic", title="Kiln Store")
        self.stack.add_named(scroll, "list"); self.stack.add_named(self.status, "status")
        tv.set_content(self.stack)
        self.toasts.set_child(tv)
        self.set_content(self.toasts)
        self.reload()
        GLib.idle_add(self.search.grab_focus)
        threading.Thread(target=self.bootstrap, daemon=True).start()

    # ---- startup: connectivity, appstream, stale apt lists (all off the UI thread)
    def bootstrap(self):
        on = backends.online()
        if not on:
            GLib.idle_add(self.show_banner, "Offline: showing installed apps and the built-in catalogue. Install needs a connection.")
            return
        if backends.flatpak_available() and not backends.flatpak_have_appstream():
            GLib.idle_add(self.show_banner, "Fetching the Flathub catalogue...")
            backends.ensure_polkit_agent()
            ok = backends.flatpak_refresh_appstream()
            GLib.idle_add(self.show_banner, None if ok else "Could not fetch the Flathub catalogue; using the web search fallback.")
            if ok:
                GLib.idle_add(self.reload)
        if backends.apt_lists_empty():
            GLib.idle_add(self.show_banner, "Debian package lists are empty. Run 'sudo apt update' (Update all in the Updates tab does it too).")

    def show_banner(self, text):
        self.banner.set_title(text or ""); self.banner.set_revealed(bool(text))
        return False

    # ---- listing
    def clear(self):
        while (r := self.list.get_first_child()) is not None:
            self.list.remove(r)

    def fill(self, pkgs, empty="No results."):
        self.clear()
        if not pkgs:
            self.status.set_title(empty); self.status.set_description("")
            self.stack.set_visible_child_name("status"); return
        for p in pkgs:
            self.list.append(Row(p, self.act, self.open_details))
        self.stack.set_visible_child_name("list")

    def work(self, fn, empty):
        """Run fn on a thread; show results only if still the latest request."""
        self.gen += 1
        gen = self.gen
        self.status.set_title("Working..."); self.status.set_description("")
        self.stack.set_visible_child_name("status")
        def run():
            try:
                res = fn()
            except Exception as e:  # never leave the spinner up
                res = []; GLib.idle_add(self.toasts.add_toast, Adw.Toast.new(f"Error: {e}"))
            GLib.idle_add(lambda: (self.fill(res, empty) if gen == self.gen else None) or False)
        threading.Thread(target=run, daemon=True).start()

    def reload(self, *_):
        self.update_all.set_visible(False)
        self.cat_scroll.set_visible(self.view_name == "browse" and not self.search.get_text().strip())
        if self.view_name == "installed":
            self.work(lambda: backends.apt_installed_apps() + backends.flatpak_installed_apps(), "No installed apps found.")
        elif self.view_name == "updates":
            def fn():
                return backends.apt_upgradable() + backends.flatpak_updates()
            self.work(fn, "Everything is up to date.")
            self.update_all.set_visible(True)
        elif self.search.get_text().strip():
            q, sel = self.search.get_text().strip(), self.mode.get_selected()
            self.work(lambda: combined_search(q, sel), f"No results for \u201c{q}\u201d.")
        else:
            cat = self.category
            self.work(lambda: backends.category_items(cat), "Nothing to show.")

    def tab_toggled(self, btn, key):
        if btn.get_active() and key != self.view_name:
            self.view_name = key
            self.reload()

    def cat_toggled(self, btn, cat):
        if btn.get_active() and cat != self.category:
            self.category = cat
            self.reload()

    def search_changed(self, entry):
        if self.timeout:
            GLib.source_remove(self.timeout)
        self.timeout = GLib.timeout_add(450, self.do_search)

    def do_search(self, *_):
        self.timeout = 0
        if self.view_name != "browse":
            self.tabs["browse"].set_active(True)   # triggers reload via tab_toggled
            if self.view_name == "browse":
                return False
        self.reload()
        return False

    # ---- details dialog
    def open_details(self, pkg):
        dlg = Adw.Dialog(title=pkg.name, content_width=520, content_height=460)
        tv = Adw.ToolbarView(); tv.add_top_bar(Adw.HeaderBar())
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        for m in ("top", "bottom", "start", "end"):
            getattr(box, f"set_margin_{m}")(18)
        info = Gtk.Label(label="Loading...", xalign=0, wrap=True, selectable=True, yalign=0, vexpand=True)
        title = Gtk.Label(label=pkg.name, xalign=0, css_classes=["title-2"])
        meta = Gtk.Label(label=("Flathub" if pkg.source == "flatpak" else "Debian") + "  |  " + pkg.pid, xalign=0, css_classes=["dim-label", "caption"])
        box.append(title); box.append(meta); box.append(info)
        sc = Gtk.ScrolledWindow(vexpand=True); sc.set_child(box); tv.set_content(sc)
        dlg.set_child(tv); dlg.present(self)
        def fn():
            if pkg.source == "apt":
                ver, desc = backends.apt_details(pkg.pid)
                return f"Version: {ver}\n\n{desc or pkg.summary}"
            d = backends.flatpak_details(pkg)
            return "\n".join(x for x in (f"Version: {d['version']}" if d["version"] else "", f"Size: {d['size']}" if d["size"] else "",
                                          f"License: {d['license']}" if d["license"] else "", "", d["text"]) if x is not None)
        def run():
            try:
                txt = fn() or pkg.summary
            except Exception:
                txt = pkg.summary
            GLib.idle_add(lambda: info.set_label(txt) or False)
        threading.Thread(target=run, daemon=True).start()

    # ---- install / remove / update
    def run_cmd(self, cmd, label, done):
        if cmd is None:
            self.toasts.add_toast(Adw.Toast.new(backends.NO_PRIV)); done(False); return
        backends.ensure_polkit_agent()
        def run():
            try:
                r = subprocess.run(cmd, capture_output=True, text=True, timeout=3600)
                ok, msg = r.returncode == 0, (r.stderr or r.stdout).strip().splitlines()[-1:] or [""]
            except (OSError, subprocess.TimeoutExpired) as e:
                ok, msg = False, [str(e)]
            def fin():
                self.toasts.add_toast(Adw.Toast.new(f"{label} done" if ok else f"{label} failed: {msg[0][:140]}"))
                done(ok); return False
            GLib.idle_add(fin)
        threading.Thread(target=run, daemon=True).start()

    def act(self, row, action):
        row.busy(True)
        pkg = row.pkg
        def start():
            if pkg.source == "flatpak" and action == "install":
                backends.flatpak_ensure_remote()
            cmd = backends.command_for(action, pkg)
            GLib.idle_add(lambda: self.run_cmd(cmd, f"{pkg.name} {action}", after) or False)
        def after(ok):
            row.busy(False)
            if ok:
                row.pkg.installed = action == "install"
                row.refresh()
                if self.view_name == "installed" and action == "remove":
                    self.list.remove(row.get_parent())
        threading.Thread(target=start, daemon=True).start()

    def do_update_all(self, btn):
        btn.set_sensitive(False)
        names = []
        child = self.list.get_first_child()
        fp = False
        while child is not None:
            row = child.get_child() if hasattr(child, "get_child") else child
            if isinstance(row, Row):
                if row.pkg.source == "apt":
                    names.append(row.pkg.pid)
                else:
                    fp = True
            child = child.get_next_sibling()
        steps = []
        if names:
            steps.append((backends.apt_update_cmd(), "Refreshing Debian lists"))
            steps.append((backends.apt_upgrade_cmd(names), "Debian updates"))
        if fp:
            steps.append((backends.flatpak_update_cmd(), "Flatpak updates"))
        def nxt(ok=True):
            if steps:
                cmd, label = steps.pop(0)
                self.run_cmd(cmd, label, nxt)
            else:
                btn.set_sensitive(True); self.reload()
        nxt()


class App(Adw.Application):
    def __init__(self):
        super().__init__(application_id=APP_ID, flags=Gio.ApplicationFlags.DEFAULT_FLAGS)
        self.add_action(self._action("about", self.about))
        self.add_action(self._action("quit", lambda *_: self.quit()))
        self.set_accels_for_action("app.quit", ["<Primary>q"])

    def _action(self, name, cb):
        a = Gio.SimpleAction.new(name, None); a.connect("activate", cb); return a

    def do_startup(self):
        Adw.Application.do_startup(self)
        load_theme()

    def do_activate(self):
        win = self.props.active_window or Window(self)
        win.present()

    def about(self, *_):
        d = Adw.AboutDialog(application_name="Kiln Store", application_icon=APP_ID, developer_name="Kiln GNU/Linux",
                            version=VERSION, comments="Browse and install apps from Debian and Flathub.\nKiln GNU/Linux",
                            license_type=Gtk.License.GPL_3_0)
        d.present(self.props.active_window)


def main(argv):
    GLib.set_prgname(APP_ID)
    GLib.set_application_name("Kiln Store")
    return App().run(argv)
