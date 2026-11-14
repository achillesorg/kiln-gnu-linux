#!/usr/bin/env python3
"""Fill the Kiln design tokens into theme/*.css.in. Usage: gen-css.py TEMPLATE OUT_LIGHT OUT_DARK"""
import sys, re
LIGHT = dict(bg='#FAF9F5', base='#FFFFFF', panel='#F5F2EA', raised='#FCFBF7', sunken='#EFEBE0', line='#CFC6B4',
             ink='#2B2926', muted='#6B665C', accent='#D97757', accent_hi='#E38A6C', sel_bg='#F2D5C6', sel_fg='#2B2926',
             hover='#F0EBDF', danger='#B03A2E', accent_text='#B5502C', ink_bg='#2B2926', ink_fg='#F3EFE6')
DARK = dict(bg='#2B2926', base='#24221F', panel='#2B2926', raised='#33302C', sunken='#24221F', line='#4A4640',
            ink='#F3EFE6', muted='#B3AC9F', accent='#D97757', accent_hi='#E38A6C', sel_bg='#584035', sel_fg='#F3EFE6',
            hover='#3A3631', danger='#C8473A', accent_text='#EE8A7C', ink_bg='#F3EFE6', ink_fg='#2B2926')
t = open(sys.argv[1]).read()
for toks, out in ((LIGHT, sys.argv[2]), (DARK, sys.argv[3])):
    s = re.sub(r'@([a-z_]+)\b', lambda m: toks[m.group(1)] if m.group(1) in toks else m.group(0), t)
    open(out, 'w').write(s)
