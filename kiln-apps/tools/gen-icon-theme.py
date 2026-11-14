#!/usr/bin/env python3
"""Generate the 'Kiln' icon theme (inherits Adwaita): flat paper folders and a few place icons."""
import os
root = os.path.join(os.path.dirname(__file__), '..', 'icon-theme', 'Kiln')
INK, TERRA, SAND, LINE = '#2B2926', '#D97757', '#F2D5C6', '#CFC6B4'
S = f'stroke="{INK}" stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round"'
def folder(glyph=''):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">'
            f'<path d="M8 14h18l5 6h25v34H8z" fill="#E7B79F" {S}/>'
            f'<path d="M8 26h48v28H8z" fill="{SAND}" {S}/>{glyph}</svg>\n')
def gl(d, fill='none', c=INK): return f'<path d="{d}" fill="{fill}" stroke="{c}" stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round"/>'
icons = {
 'folder': folder(), 'folder-open': folder(), 'inode-directory': folder(),
 'user-home': folder(gl('M22 42l10-9 10 9M25 40v8h14v-8', TERRA)),
 'user-desktop': folder(gl('M22 34h20v10H22zM28 48h8', TERRA)),
 'folder-documents': folder(gl('M26 33h12M26 39h12M26 45h8', TERRA)),
 'folder-download': folder(gl('M32 32v12M27 40l5 5 5-5', TERRA)),
 'folder-pictures': folder(gl('M23 46l7-9 5 5 4-4 5 8z', TERRA)),
 'folder-music': folder(gl('M35 34v11M35 34l6-2M31 46a3 3 0 1 0 0-.1', TERRA)),
 'folder-videos': folder(gl('M28 33l10 6-10 6z', TERRA)),
 'folder-publicshare': folder(gl('M32 32l3 6h6l-5 4 2 6-6-4-6 4 2-6-5-4h6z', TERRA)),
 'folder-templates': folder(gl('M24 34h16v12H24zM24 40h16', TERRA)),
 'folder-remote': folder(gl('M24 44a6 6 0 0 1 4-10 8 8 0 0 1 14 3 4 4 0 0 1-2 7z', TERRA)),
 'user-trash': ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">'
                f'<path d="M16 20h32l-3 36H19z" fill="{SAND}" {S}/><path d="M12 20h40M26 20v-6h12v6" fill="none" {S}/>'
                f'<path d="M27 28v20M37 28v20" fill="none" stroke="{TERRA}" stroke-width="2.5" stroke-linecap="round"/></svg>\n'),
 'drive-harddisk': ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">'
                f'<rect x="8" y="20" width="48" height="26" rx="4" fill="{SAND}" {S}/><circle cx="46" cy="38" r="2.5" fill="{TERRA}"/><path d="M14 38h18" {S}/></svg>\n'),
}
icons['computer'] = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">'
                f'<rect x="8" y="12" width="48" height="32" rx="4" fill="{SAND}" {S}/><path d="M24 54h16M32 44v10" {S}/><path d="M16 36l8-8 6 5 8-9 8 9" fill="none" stroke="{TERRA}" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/></svg>\n')
icons['document-open-recent'] = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">'
                f'<circle cx="32" cy="32" r="22" fill="{SAND}" {S}/><path d="M32 18v14l9 6" fill="none" stroke="{TERRA}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/></svg>\n')
icons['network-workgroup'] = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">'
                f'<circle cx="32" cy="32" r="22" fill="{SAND}" {S}/><path d="M10 32h44M32 10c-12 12-12 32 0 44M32 10c12 12 12 32 0 44" fill="none" stroke="{TERRA}" stroke-width="2.5"/></svg>\n')
icons['network-server'] = icons['network-workgroup']
icons['go-home'] = icons['user-home']
icons['user-trash-full'] = icons['user-trash']
icons['drive-removable-media'] = icons['drive-harddisk']
d = os.path.join(root, 'scalable', 'places'); os.makedirs(d, exist_ok=True)
for n, s in icons.items():
    if n == 'inode-directory': continue
    open(os.path.join(d, n + '.svg'), 'w').write(s)
m = os.path.join(root, 'scalable', 'mimetypes'); os.makedirs(m, exist_ok=True)
open(os.path.join(m, 'inode-directory.svg'), 'w').write(icons['inode-directory'])
dev = os.path.join(root, 'scalable', 'devices'); os.makedirs(dev, exist_ok=True)
for n in ('drive-harddisk', 'drive-removable-media', 'computer'): open(os.path.join(dev, n + '.svg'), 'w').write(icons[n])
for n in ('document-open-recent', 'network-workgroup', 'network-server'): open(os.path.join(d, n + '.svg'), 'w').write(icons[n])
ap = os.path.join(root, 'scalable', 'apps'); os.makedirs(ap, exist_ok=True)
for f in os.listdir(os.path.join(os.path.dirname(__file__), '..', 'icons')):
    if f.endswith('.svg'): open(os.path.join(ap, f), 'w').write(open(os.path.join(os.path.dirname(__file__), '..', 'icons', f)).read())
open(os.path.join(root, 'index.theme'), 'w').write('''[Icon Theme]
Name=Kiln
Comment=Kiln GNU/Linux flat paper icons (folders and places); falls back to Adwaita
Inherits=Adwaita,hicolor
Directories=scalable/places,scalable/mimetypes,scalable/devices,scalable/apps

[scalable/places]
Size=64
MinSize=8
MaxSize=512
Type=Scalable
Context=Places

[scalable/mimetypes]
Size=64
MinSize=8
MaxSize=512
Type=Scalable
Context=MimeTypes

[scalable/devices]
Size=64
MinSize=8
MaxSize=512
Type=Scalable
Context=Devices

[scalable/apps]
Size=64
MinSize=8
MaxSize=512
Type=Scalable
Context=Applications
''')
print('ok')
