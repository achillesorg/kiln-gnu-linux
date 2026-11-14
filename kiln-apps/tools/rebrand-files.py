#!/usr/bin/env python3
"""Rebrand a Thunar 4.20.x checkout in place as Kiln Files. Run once on a pristine tree.
Idempotent enough: second run is a no-op."""
import os, re, sys
root = sys.argv[1]
SKIP_DIRS = {'.git', 'po', 'docs'}
SKIP_FILES = {'COPYING', 'COPYING.LIB', 'NEWS', 'ChangeLog.gio-migration', 'AUTHORS', 'THANKS', 'README.md'}
lit = re.compile(r'"(?:[^"\\\n]|\\.)*"')
word = re.compile(r'(?<![A-Za-z_0-9\-./])Thunar(?![A-Za-z_0-9])')
ids = [('org.xfce.thunar', 'org.kiln.Files'), ('org.xfce.Thunar', 'org.kiln.Files'),
       ('org.xfce.FileManager', 'org.kiln.FileManager')]
def fix_lit(m):
    s = m.group(0)
    if s in ('"Thunar"',) : return '"Kiln Files"'
    return word.sub('Kiln Files', s)
for d, ds, fs in os.walk(root):
    ds[:] = [x for x in ds if x not in SKIP_DIRS]
    for f in fs:
        if f in SKIP_FILES: continue
        p = os.path.join(d, f)
        try: t = open(p, encoding='utf-8').read()
        except Exception: continue
        o = t
        for a, b in ids: t = t.replace(a, b)
        if f.endswith(('.c', '.h')):
            t = lit.sub(fix_lit, t)
        elif f.endswith(('.in', '.xml', '.ui', '.policy')) or f.startswith('thunar-settings'):
            t = word.sub('Kiln Files', t)
        if f.endswith('.desktop.in.in') or f.endswith('.service.in'):
            t = re.sub(r'(Exec(?:Start)?=(?:@bindir@/)?)(?:T|t)hunar(?:-settings)?(?=[ \n])', lambda m: m.group(1) + 'kiln-files', t)
        if t != o: open(p, 'w', encoding='utf-8').write(t)
# file renames
for d, ds, fs in os.walk(root):
    ds[:] = [x for x in ds if x not in SKIP_DIRS]
    for f in fs:
        n = f
        for a, b in ids: n = n.replace(a, b)
        if n != f: os.rename(os.path.join(d, f), os.path.join(d, n))
print('done')
