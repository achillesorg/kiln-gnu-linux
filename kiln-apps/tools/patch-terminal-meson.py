import re, os
t = open("meson.build").read()
t = t.replace("executable(\n  'foot',", "executable(\n  'kiln-terminal',", 1)
t = t.replace("  'foot-features.c', 'foot-features.h',\n  'ime.c'", "  'foot-features.c', 'foot-features.h',\n  'kiln-colours.c', 'kiln-colours.h',\n  'ime.c'", 1)
t = t.replace("executable(\n  'footclient',", "executable(\n  'kiln-terminal-client',", 1)
t = t.replace("  'foot.desktop', 'foot-server.desktop', 'footclient.desktop',", "  'org.kiln.Terminal.desktop',", 1)
t = t.replace("install_data('foot.ini', install_dir: join_paths(get_option('sysconfdir'), 'xdg', 'foot'))",
  "install_data('kiln-terminal.ini', install_dir: join_paths(get_option('sysconfdir'), 'xdg', 'kiln-terminal'))\n"
  "install_data('org.kiln.Terminal.svg', install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', 'scalable', 'apps'))\n"
  "install_data('kiln-terminal-scheme', install_dir: get_option('bindir'), install_mode: 'rwxr-xr-x')", 1)
t = t.replace("subdir('completions')\nsubdir('icons')\nsubdir('utils')\n", "# Kiln: completions, icons and utils of upstream are not installed\n", 1)
t = t.replace("project('foot', 'c',", "project('kiln-terminal', 'c',", 1)
open("meson.build", "w").write(t)
os.rename("foot.ini", "kiln-terminal.ini")
for f in ("foot.desktop", "footclient.desktop", "foot-server.desktop", "foot-server.service.in", "foot-server.socket"):
    os.remove(f)
