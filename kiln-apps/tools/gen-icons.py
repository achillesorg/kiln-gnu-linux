#!/usr/bin/env python3
"""Generate flat cut-paper app icons (SVG) for the Kiln apps into ../icons/."""
import os
out = os.path.join(os.path.dirname(__file__), '..', 'icons')
INK, TERRA, CREAM, LINE, SAND = '#2B2926', '#D97757', '#FAF9F5', '#CFC6B4', '#F2D5C6'
tile = f'<rect x="6" y="6" width="116" height="116" rx="22" fill="{CREAM}" stroke="{LINE}" stroke-width="3"/>'
def st(w=5, c=INK): return f'fill="none" stroke="{c}" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round"'
G = {
'Files': f'<path d="M26 40h28l8 10h40v46H26z" fill="{SAND}" {st()[len("fill=\"none\" "):]}/><path d="M26 56h76v40H26z" fill="{TERRA}" stroke="{INK}" stroke-width="5" stroke-linejoin="round"/>',
'Terminal': f'<rect x="22" y="30" width="84" height="68" rx="8" fill="{INK}"/><path d="M38 52l16 12-16 12" {st(6, TERRA)}/><path d="M62 78h26" {st(6, CREAM)}/>',
'Text': f'<path d="M34 24h42l20 20v60H34z" fill="#FFF" stroke="{INK}" stroke-width="5" stroke-linejoin="round"/><path d="M76 24v20h20" {st()}/><path d="M46 62h40M46 76h40M46 90h24" {st(5, TERRA)}/>',
'Viewer': f'<rect x="22" y="30" width="84" height="68" rx="6" fill="#FFF" stroke="{INK}" stroke-width="5"/><circle cx="82" cy="52" r="9" fill="{TERRA}"/><path d="M22 90l24-28 18 20 12-12 30 22v6H22z" fill="{SAND}" stroke="{INK}" stroke-width="5" stroke-linejoin="round"/>',
'Calculator': f'<rect x="30" y="20" width="68" height="88" rx="8" fill="#FFF" stroke="{INK}" stroke-width="5"/><rect x="40" y="30" width="48" height="18" rx="3" fill="{SAND}" stroke="{INK}" stroke-width="3"/><g fill="{INK}"><circle cx="46" cy="64" r="4"/><circle cx="64" cy="64" r="4"/><circle cx="82" cy="64" r="4"/><circle cx="46" cy="80" r="4"/><circle cx="64" cy="80" r="4"/></g><rect x="74" y="74" width="16" height="26" rx="4" fill="{TERRA}" stroke="{INK}" stroke-width="3"/><circle cx="46" cy="96" r="4" fill="{INK}"/><circle cx="64" cy="96" r="4" fill="{INK}"/>',
'Monitor': f'<rect x="20" y="28" width="88" height="62" rx="6" fill="#FFF" stroke="{INK}" stroke-width="5"/><path d="M28 64h18l8-20 14 34 8-14h16" {st(5, TERRA)}/><path d="M50 104h28M64 90v14" {st()}/>',
'Store': f'<path d="M30 48h68l-6 56H36z" fill="{SAND}" stroke="{INK}" stroke-width="5" stroke-linejoin="round"/><path d="M48 48a16 16 0 0 1 32 0" {st()}/><path d="M52 70l12 12 12-12" {st(5, TERRA)}/>',
}
for n, g in G.items():
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" width="128" height="128"><title>Kiln {n}</title>{tile}{g}</svg>\n'
    open(os.path.join(out, f'org.kiln.{n}.svg'), 'w').write(svg)
print('ok')
