#!/usr/bin/env python3
"""Kiln Monitor patches on a rebranded gnome-system-monitor 48.1 tree."""
def sub(p, a, b, n=1):
    t = open(p).read(); assert a in t, (p, a); open(p, 'w').write(t.replace(a, b, n))
sub('src/application.cpp', '  adw_style_manager_set_color_scheme (style_manager, ADW_COLOR_SCHEME_PREFER_LIGHT);', '  /* Kiln: follow the system colour scheme (upstream forced prefer-light) */\n  kiln_theme_init ();')
sub('src/application.cpp', 'void\nGsmApplication::on_startup ()', 'extern "C" void kiln_theme_init (void);\n\nvoid\nGsmApplication::on_startup ()')
sub('src/interface.cpp', '  adw_about_dialog_set_developers (ADW_ABOUT_DIALOG (about_dialog), authors);', '  adw_about_dialog_set_developer_name (ADW_ABOUT_DIALOG (about_dialog), "Kiln GNU/Linux");\n  adw_about_dialog_set_comments (ADW_ABOUT_DIALOG (about_dialog), _("Based on GNOME System Monitor 48.1 by the GNOME Project."));\n  adw_about_dialog_set_version (ADW_ABOUT_DIALOG (about_dialog), VERSION "-kiln1");\n  adw_about_dialog_set_developers (ADW_ABOUT_DIALOG (about_dialog), authors);')
sub('src/meson.build', "system_monitor_sources = [", "system_monitor_sources = [\n  'kiln-theme-adw.c',")
t = open('src/meson.build').read()
t = t.replace("executable(meson.project_name(),", "install_data(['kiln-light.css', 'kiln-dark.css'], install_dir: join_paths(get_option('prefix'), get_option('datadir'), 'kiln-monitor'))\nexecutable(meson.project_name(),", 1)
open('src/meson.build', 'w').write(t)
sub('data/org.kiln.Monitor.desktop.in.in', 'StartupWMClass=kiln-monitor', 'StartupWMClass=org.kiln.Monitor')
sub('data/org.kiln.Monitor.desktop.in.in', 'Categories=GNOME;GTK;', 'Categories=GTK;')
open('data/org.kiln.Monitor.metainfo.xml.in.in', 'w').write('''<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>@APP_ID@</id>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>GPL-2.0-or-later</project_license>
  <name>Kiln Monitor</name>
  <summary>View current processes and monitor system state</summary>
  <developer id="org.kiln"><name>Kiln GNU/Linux</name></developer>
  <description><p>Kiln Monitor shows processes, resources and disks of Kiln GNU/Linux. It is a fork of GNOME System Monitor.</p></description>
  <launchable type="desktop-id">@APP_ID@.desktop</launchable>
  <content_rating type="oars-1.1"/>
  <releases><release version="48.1" date="2025-05-01"/></releases>
</component>
''')
sub('src/meson.build', "  cpp_args: [\n    '-DGSM_RESOURCE_FILE", "  c_args: ['-DKILN_THEME_DIR=\"@0@\"'.format(join_paths(get_option('prefix'), get_option('datadir'), 'kiln-monitor'))],\n  cpp_args: [\n    '-DGSM_RESOURCE_FILE")
