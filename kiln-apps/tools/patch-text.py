#!/usr/bin/env python3
"""Kiln Text patches on a rebranded gnome-text-editor 48.3 tree (run in tree root)."""
import re
def sub(p, a, b, n=1):
    t = open(p).read(); assert a in t, (p, a); open(p, 'w').write(t.replace(a, b, n))
sub('src/editor-application.c', 'editor_application_startup (GApplication *application)\n{', 'editor_application_startup (GApplication *application)\n{\n  extern void kiln_theme_init (void);')
sub('src/editor-application.c', '  EditorApplication *self = (EditorApplication *)application;\n  GtkSourceStyleSchemeManager *schemes;', '  EditorApplication *self = (EditorApplication *)application;\n  GtkSourceStyleSchemeManager *schemes;', 1)
t = open('src/editor-application.c').read()
i = t.index('editor_application_startup (GApplication *application)')
j = t.index('G_APPLICATION_CLASS (editor_application_parent_class)->startup', i)
t = t[:j] + 'kiln_theme_init ();\n  ' + t[j:]
open('src/editor-application.c', 'w').write(t)
sub('src/meson.build', "editor_sources = [\n", "editor_sources = [\n  'kiln-theme-adw.c',\n")
sub('src/meson.build', 'editor_c_args = []', "editor_c_args = ['-DKILN_THEME_DIR=\"@0@\"'.format(join_paths(get_option('prefix'), get_option('datadir'), 'kiln-text'))]")
sub('src/editor-application-actions.c', '"developer-name", _("The GNOME Project"),', '"developer-name", "Kiln GNU/Linux",\n                         "comments", _("Based on GNOME Text Editor 48.3 by the GNOME Project."),')
sub('src/editor-application-actions.c', '"version", PACKAGE_VERSION,', '"version", PACKAGE_VERSION "-kiln1",')
sub('data/org.kiln.Text.gschema.xml', "<default>'Adwaita'</default>", "<default>'kiln'</default>")
sub('data/meson.build', "gnome.compile_schemas(build_by_default: true)", "install_data(['kiln-light.css', 'kiln-dark.css'], install_dir: join_paths(get_option('datadir'), 'kiln-text'))\ninstall_data(['styles/kiln.xml', 'styles/kiln-dark.xml'], install_dir: join_paths(get_option('datadir'), 'gtksourceview-5', 'styles'))\ngnome.compile_schemas(build_by_default: true)")
t = open('data/org.kiln.Text.desktop.in.in').read().replace('Categories=GNOME;GTK;', 'Categories=GTK;')
if 'StartupWMClass' not in t: t = t.replace('[Desktop Entry]\n', '[Desktop Entry]\nStartupWMClass=@app_id@\n', 1)
open('data/org.kiln.Text.desktop.in.in', 'w').write(t)
open('data/org.kiln.Text.metainfo.xml.in.in', 'w').write('''<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>@app_id@</id>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>GPL-3.0-or-later</project_license>
  <name>Kiln Text</name>
  <summary>Edit text files</summary>
  <developer id="org.kiln"><name>Kiln GNU/Linux</name></developer>
  <description><p>Kiln Text is the text editor of Kiln GNU/Linux, a fork of GNOME Text Editor.</p></description>
  <launchable type="desktop-id">@app_id@.desktop</launchable>
  <content_rating type="oars-1.1"/>
  <provides><binary>kiln-text</binary></provides>
</component>
''')
