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
FEATURED = ["firefox", "thunderbird", "libreoffice", "vlc", "gimp", "inkscape", "krita", "blender", "audacity", "obs-studio",
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


class Row(Gtk.Box):
    def __init__(self, pkg, on_action):
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
        super().__init__(application=app, title="Kiln Store", default_width=820, default_height=640)
        self.source = "all"
        self.toasts = Adw.ToastOverlay()
        view = Adw.ToolbarView()
        header = Adw.HeaderBar()
        self.search = Gtk.SearchEntry(placeholder_text="Search apps (Debian and Flathub)", hexpand=True, width_chars=40)
        self.search.connect("activate", self.do_search)
        self.search.connect("search-changed", self.search_changed)
        header.set_title_widget(self.search)
        self.mode = Gtk.DropDown.new_from_strings(["All sources", "Debian (apt)", "Flathub"])
        self.mode.connect("notify::selected", lambda *_: self.do_search())
        header.pack_end(self.mode)
        tabs = Gtk.ToggleButton(label="Installed")
        tabs.connect("toggled", self.toggle_installed)
        header.pack_start(tabs)
        view.add_top_bar(header)
        self.stack = Adw.ViewStack()
        self.list = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
        self.list.add_css_class("kiln-plain")
        scroll = Gtk.ScrolledWindow(vexpand=True)
        clamp = Adw.Clamp(maximum_size=760)
        clamp.set_margin_top(12); clamp.set_margin_bottom(12); clamp.set_margin_start(16); clamp.set_margin_end(16)
        clamp.set_child(self.list); scroll.set_child(clamp)
        self.status = Adw.StatusPage(icon_name="system-software-install-symbolic", title="Kiln Store", description="Search for an app, or browse the suggestions.")
        self.stack.add_named(scroll, "list"); self.stack.add_named(self.status, "status")
        view.set_content(self.stack)
        self.toasts.set_child(view)
        self.set_content(self.toasts)
        self.timeout = 0
        self.installed_mode = False
        self.show_featured()
        # the search entry has focus on open
        GLib.idle_add(self.search.grab_focus)

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
            self.list.append(Row(p, self.act))
        self.stack.set_visible_child_name("list")

    def work(self, fn, done):
        self.status.set_title("Working..."); self.status.set_description("")
        self.stack.set_visible_child_name("status")
        def run():
            res = fn()
            GLib.idle_add(done, res)
        threading.Thread(target=run, daemon=True).start()

    def show_featured(self):
        def fn():
            inst = backends.apt_installed_set()
            rc, o, _ = backends._run(["apt-cache", "show"] + FEATURED, 60)   # one call: much faster than one per package
            out, seen = [], set()
            for stanza in o.split("\n\n"):
                name = summ = ""
                for l in stanza.splitlines():
                    if l.startswith("Package:"):
                        name = l.split(":", 1)[1].strip()
                    elif (l.startswith("Description:") or l.startswith("Description-en:")) and not summ:
                        summ = l.split(":", 1)[1].strip()
                if name and name not in seen:
                    seen.add(name)
                    out.append(backends.Pkg("apt", name, name, summ, name in inst))
            return out
        self.work(fn, lambda r: self.fill(r, "Nothing to show.") or False)

    def search_changed(self, entry):
        if self.timeout:
            GLib.source_remove(self.timeout)
        self.timeout = GLib.timeout_add(500, self.do_search)

    def do_search(self, *_):
        self.timeout = 0
        q = self.search.get_text().strip()
        if self.installed_mode:
            return False
        if not q:
            self.show_featured(); return False
        sel = self.mode.get_selected()
        def fn():
            res = []
            if sel in (0, 1):
                res += backends.apt_search(q)
            if sel in (0, 2):
                res += backends.flatpak_search(q)
            return res
        self.work(fn, lambda r: self.fill(r, f"No results for “{q}”.") or False)
        return False

    def toggle_installed(self, btn):
        self.installed_mode = btn.get_active()
        if self.installed_mode:
            self.work(lambda: backends.apt_installed_apps() + backends.flatpak_installed_apps(), lambda r: self.fill(r, "No installed apps found.") or False)
        else:
            self.do_search()

    # ---- install / remove
    def act(self, row, action):
        row.busy(True)
        pkg = row.pkg
        def run():
            if pkg.source == "flatpak" and action == "install":
                backends.flatpak_ensure_remote()
            cmd = backends.command_for(action, pkg)
            try:
                r = subprocess.run(cmd, capture_output=True, text=True, timeout=3600)
                ok, msg = r.returncode == 0, (r.stderr or r.stdout).strip().splitlines()[-1:] or [""]
            except (OSError, subprocess.TimeoutExpired) as e:
                ok, msg = False, [str(e)]
            GLib.idle_add(self.finished, row, action, ok, msg[0])
        threading.Thread(target=run, daemon=True).start()

    def finished(self, row, action, ok, msg):
        row.busy(False)
        if ok:
            row.pkg.installed = action == "install"
            row.refresh()
            self.toasts.add_toast(Adw.Toast.new(f"{row.pkg.name} {'installed' if action == 'install' else 'removed'}"))
        else:
            self.toasts.add_toast(Adw.Toast.new(f"{action.capitalize()} failed: {msg[:120]}"))
        return False


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
