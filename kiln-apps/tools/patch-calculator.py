#!/usr/bin/env python3
"""Kiln Calculator patches on a rebranded gnome-calculator 48.1 tree."""
def sub(p, a, b, n=1):
    t = open(p).read(); assert a in t, (p, a); open(p, 'w').write(t.replace(a, b, n))
sub('src/kiln-calculator.vala', 'application_name = _("Calculator"),', 'application_name = "Kiln Calculator",')
sub('src/kiln-calculator.vala', 'developer_name = _("The GNOME Project"),', 'developer_name = "Kiln GNU/Linux",\n            comments = _("Based on GNOME Calculator 48.1 by the GNOME Project."),')
sub('src/kiln-calculator.vala', 'version =  VERSION,', 'version =  VERSION + "-kiln1",')
sub('src/kiln-calculator.vala', 'current_window.set_title (_("Calculator"));', 'current_window.set_title ("Kiln Calculator");')
sub('src/kiln-calculator.vala', '        base.startup ();', '        base.startup ();\n        kiln_theme_init ();')
t = open('src/kiln-calculator.vala').read()
i = t.index('class Calculator')
t = t[:i] + '[CCode (cname = "kiln_theme_init", cheader_filename = "kiln-theme.h")]\nextern void kiln_theme_init ();\n\n' + t[i:]
open('src/kiln-calculator.vala', 'w').write(t)
open('src/kiln-theme.h', 'w').write('#pragma once\nvoid kiln_theme_init (void);\n')
sub('src/meson.build', "  'kiln-calculator.vala',", "  'kiln-calculator.vala',\n  'kiln-theme-adw.c',")
sub('src/meson.build', "gnome_calculator_c_flags = [", "gnome_calculator_c_flags = [\n  '-DKILN_THEME_DIR=\"@0@\"'.format(join_paths(get_option('prefix'), get_option('datadir'), 'kiln-calculator')),")
sub('src/meson.build', "executable('kiln-calculator', gnome_calculator_sources,", "install_data(['kiln-light.css', 'kiln-dark.css'], install_dir: join_paths(get_option('datadir'), 'kiln-calculator'))\nexecutable('kiln-calculator', gnome_calculator_sources,")
t = open('meson.build').read()
for d in ("subdir('search-provider')", "subdir('help')", "subdir('doc')", "subdir('tests')"):
    t = t.replace(d, '# Kiln: ' + d)
open('meson.build', 'w').write(t)
t = open('data/org.kiln.Calculator.desktop.in').read()
t = t.replace('Name=Calculator', 'Name=Kiln Calculator').replace('Categories=GNOME;GTK;', 'Categories=GTK;')
if 'StartupWMClass' not in t: t = t.replace('Terminal=false', 'Terminal=false\nStartupWMClass=@app_id@', 1)
open('data/org.kiln.Calculator.desktop.in', 'w').write(t)
open('data/org.kiln.Calculator.metainfo.xml.in', 'w').write('''<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>@app_id@</id>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>GPL-3.0-or-later</project_license>
  <name>Kiln Calculator</name>
  <summary>Perform arithmetic, scientific or financial calculations</summary>
  <developer id="org.kiln"><name>Kiln GNU/Linux</name></developer>
  <description><p>Kiln Calculator is the calculator of Kiln GNU/Linux, a fork of GNOME Calculator.</p></description>
  <launchable type="desktop-id">@app_id@.desktop</launchable>
  <content_rating type="oars-1.1"/>
</component>
''')
