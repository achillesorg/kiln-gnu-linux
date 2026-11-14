#!/usr/bin/env python3
"""rebrand-gnome.py TREE OLD_ID NEW_ID OLD_BIN NEW_BIN OLD_NAME NEW_NAME : textual rebrand of a GNOME app checkout."""
import os, sys
root, oid, nid, obin, nbin, oname, nname = sys.argv[1:8]
opath, npath = oid.replace('.', '/'), nid.replace('.', '/')
SKIP = {'.git', 'po', 'help', 'subprojects'}
SKIPF = {'COPYING', 'NEWS'}
for d, ds, fs in os.walk(root):
    ds[:] = [x for x in ds if x not in SKIP]
    for f in fs:
        if f in SKIPF: continue
        p = os.path.join(d, f)
        try: t = open(p, encoding='utf-8').read()
        except Exception: continue
        o = t
        t = t.replace(opath, npath).replace(oid, nid).replace(obin, nbin).replace(oname, nname)
        if t != o: open(p, 'w', encoding='utf-8').write(t)
for d, ds, fs in os.walk(root):
    ds[:] = [x for x in ds if x not in SKIP]
    for f in fs + ds:
        n = f.replace(oid, nid).replace(obin, nbin)
        if n != f: os.rename(os.path.join(d, f), os.path.join(d, n))
print('rebranded')
