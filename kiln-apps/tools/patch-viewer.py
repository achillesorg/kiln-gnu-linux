#!/usr/bin/env python3
"""Kiln Viewer patches on a rebranded Loupe 48.1 tree (rebrand-gnome.py run with old binary name set to a dummy)."""
import re
def sub(p, a, b, n=1):
    t = open(p).read(); assert a in t, (p, a); open(p, 'w').write(t.replace(a, b, n))
sub('src/application.rs', '''            // Force dark theme
            self.obj()
                .style_manager()
                .set_color_scheme(adw::ColorScheme::PreferDark);
''', '''            // Kiln: follow the system colour scheme (upstream forced dark) and load the Kiln theme
            crate::kiln_theme::init();
''')
open('src/kiln_theme.rs', 'w').write('''// Kiln GNU/Linux theme loader (libadwaita named-colour override, light/dark). GPL-3.0-or-later.
use crate::deps::*;
use adw::prelude::*;

pub fn init() {
    let Some(display) = gdk::Display::default() else { return };
    let provider = gtk::CssProvider::new();
    let manager = adw::StyleManager::default();
    let dir = std::env::var("KILN_THEME_DIR").unwrap_or_else(|_| "/usr/local/share/kiln-viewer".to_string());

    if let Ok(force) = std::env::var("KILN_SCHEME") {
        manager.set_color_scheme(if force == "dark" { adw::ColorScheme::ForceDark } else { adw::ColorScheme::ForceLight });
    }

    let apply = {
        let provider = provider.clone();
        move |manager: &adw::StyleManager| {
            let file = if manager.is_dark() { "kiln-dark.css" } else { "kiln-light.css" };
            provider.load_from_path(format!("{dir}/{file}"));
        }
    };
    apply(&manager);
    manager.connect_dark_notify(move |m| apply(m));
    gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
    gtk::Settings::for_display(&display).set_gtk_icon_theme_name(Some("Kiln"));
}
''')
t = open('src/meson.build').read()
t = t.replace("  output: meson.project_name(),\n  command: [\n    'cp',\n    'src' / rust_target / meson.project_name(),", "  output: 'kiln-viewer',\n  command: [\n    'cp',\n    'src' / rust_target / meson.project_name(),", 1)
open('src/meson.build', 'w').write(t)
d = open('data/meson.build').read()
d += "\ninstall_data(['kiln-light.css', 'kiln-dark.css'], install_dir: get_option('prefix') / get_option('datadir') / 'kiln-viewer')\n"
open('data/meson.build', 'w').write(d)
t = open('data/org.kiln.Viewer.desktop.in.in').read().replace('Name=Image Viewer', 'Name=Kiln Viewer')
t = re.sub(r'Exec=loupe', 'Exec=kiln-viewer', t)
if 'StartupWMClass' not in t: t = t.replace('Type=Application', 'Type=Application\nStartupWMClass=@app_id@', 1)
t = t.replace('Categories=GNOME;GTK;', 'Categories=GTK;')
open('data/org.kiln.Viewer.desktop.in.in', 'w').write(t)
sub('src/lib.rs', 'pub mod config;', 'pub mod config;\npub mod kiln_theme;')
sub('src/about.rs', '.developer_name(gettext("The GNOME Project"))', '.developer_name("Kiln GNU/Linux")\n        .comments("Based on Loupe 48.1 by the GNOME Project.")')
sub('src/about.rs', '.version(config::VERSION)', '.version(format!("{}-kiln1", config::VERSION))')
d = open('data/meson.build').read()
d = d.replace("conf.set('BIN_NAME', meson.project_name())", "conf.set('BIN_NAME', 'kiln-viewer')")
open('data/meson.build', 'w').write(d)
