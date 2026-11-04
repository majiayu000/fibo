#!/usr/bin/env python3
"""Draws each direction's layers as SVG on the full 1024 tile (the system applies the mask)."""
import math, pathlib, sys
out = pathlib.Path(sys.argv[1]); out.mkdir(parents=True, exist_ok=True)
SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">%s</svg>'

def rope_points(cx, cy, r, p_top, p_bot, step=4):
    def tangent(p, sign):
        dx, dy = p[0]-cx, p[1]-cy
        a = math.atan2(dy, dx) + sign*math.acos(r/math.hypot(dx, dy))
        return a, (cx+r*math.cos(a), cy+r*math.sin(a))
    a0, A = tangent(p_top, 1); a1, B = tangent(p_bot, -1)
    pts = []
    def line(p, q):
        n = int(math.dist(p, q)/step)
        for i in range(n): pts.append((p[0]+(q[0]-p[0])*i/n, p[1]+(q[1]-p[1])*i/n))
    line(p_top, A)
    n = int(abs(a1-a0)*r/step)
    for i in range(n):
        a = a0+(a1-a0)*i/n; pts.append((cx+r*math.cos(a), cy+r*math.sin(a)))
    line(B, p_bot)
    d = f'M{p_top[0]} {p_top[1]}L{A[0]:.1f} {A[1]:.1f}A{r} {r} 0 0 1 {B[0]:.1f} {B[1]:.1f}L{p_bot[0]} {p_bot[1]}'
    return pts, d

def rope():
    cx, cy, post = 628, 512, 112
    w = 118; r = post+26+w/2
    pts, d = rope_points(cx, cy, r, (-90, 20), (-90, 1004))
    strands = []
    every = 7
    for i in range(every, len(pts)-1, every):
        (x, y), (x2, y2) = pts[i], pts[i+1]
        L = math.hypot(x2-x, y2-y) or 1
        tx, ty = (x2-x)/L, (y2-y)/L; nx, ny = -ty, tx
        a = (x-nx*w*.56-tx*w*.30, y-ny*w*.56-ty*w*.30); b = (x+nx*w*.56+tx*w*.30, y+ny*w*.56+ty*w*.30)
        strands.append(f'M{a[0]:.1f} {a[1]:.1f}L{b[0]:.1f} {b[1]:.1f}')
    bg = '''<defs><radialGradient id="w" cx=".62" cy=".42" r=".85"><stop offset="0" stop-color="#27506a"/><stop offset=".55" stop-color="#14293f"/><stop offset="1" stop-color="#0b1524"/></radialGradient></defs>
<rect width="1024" height="1024" fill="url(#w)"/>
<g fill="none" stroke="#7fb4cc" stroke-opacity=".10" stroke-width="7" stroke-linecap="round"><path d="M90 190q60-22 120 0t120 0"/><path d="M640 130q60-22 120 0t120 0"/><path d="M700 880q60-22 120 0t120 0"/><path d="M360 934q60-22 120 0t120 0"/><path d="M120 512q50-18 100 0t100 0"/></g>'''
    fg = f'''<defs>
<linearGradient id="r" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff3d8"/><stop offset=".5" stop-color="#ecd3a4"/><stop offset="1" stop-color="#c79d68"/></linearGradient>
<mask id="e" maskUnits="userSpaceOnUse" x="-200" y="-200" width="1500" height="1500"><rect x="-200" y="-200" width="1500" height="1500" fill="#fff"/><path d="{d}" fill="none" stroke="#000" stroke-width="{w*.72}"/></mask><mask id="m" maskUnits="userSpaceOnUse" x="-200" y="-200" width="1500" height="1500"><path d="{d}" fill="none" stroke="#fff" stroke-width="{w}"/></mask>
<radialGradient id="c" cx=".38" cy=".32" r=".8"><stop offset="0" stop-color="#ff8a5c"/><stop offset=".55" stop-color="#e7502c"/><stop offset="1" stop-color="#a92f16"/></radialGradient>
<filter id="s" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx="0" dy="22" stdDeviation="20" flood-color="#000" flood-opacity=".5"/></filter>
</defs>
<circle cx="{cx}" cy="{cy}" r="{post}" fill="url(#c)" filter="url(#s)"/>
<circle cx="{cx}" cy="{cy}" r="{post-20}" fill="none" stroke="#fff" stroke-opacity=".20" stroke-width="7"/>
<g filter="url(#s)"><g mask="url(#m)"><rect x="-200" y="-200" width="1500" height="1500" fill="#efd8ad"/>
<path d="{' '.join(strands)}" stroke="url(#r)" stroke-width="7" stroke-linecap="round" fill="none" gradientUnits="userSpaceOnUse"/><path d="{d}" fill="none" stroke="#fff6e0" stroke-opacity=".35" stroke-width="{w*.34}" transform="translate(-6 -10)"/><path d="{d}" fill="none" stroke="#5c3f1f" stroke-opacity=".30" stroke-width="{w}" style="mix-blend-mode:multiply" mask="url(#e)"/></g></g>'''
    # A gradient on a bbox-less stroke needs user space; use one flat tone per strand sheet instead.
    fg = fg.replace('stroke="url(#r)"', 'stroke="#9a744a"').replace(' gradientUnits="userSpaceOnUse"', '')
    tray = lambda off: '<path d="M1 1H9L17 9L9 17H1V15H8L14 9L8 3H1Z"/>' + ('' if off else '<rect x="6" y="7" width="4" height="4"/>')
    return bg, fg, tray

def boat():
    bg = '''<defs><linearGradient id="k" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#0d1b3d"/><stop offset=".5" stop-color="#2d4f80"/><stop offset=".8" stop-color="#b9798a"/><stop offset="1" stop-color="#f4a76e"/></linearGradient>
<linearGradient id="a" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2c4d68"/><stop offset="1" stop-color="#0b1a30"/></linearGradient></defs>
<rect width="1024" height="660" fill="url(#k)"/><rect y="650" width="1024" height="374" fill="url(#a)"/>
<g fill="#fff"><circle cx="210" cy="170" r="7"/><circle cx="360" cy="110" r="5" opacity=".8"/><circle cx="800" cy="150" r="8" opacity=".9"/><circle cx="640" cy="230" r="4" opacity=".6"/><circle cx="130" cy="300" r="4" opacity=".6"/></g>
<g fill="none" stroke="#ffd28a" stroke-linecap="round"><path d="M430 720h200" stroke-width="16" opacity=".85"/><path d="M450 770h150" stroke-width="13" opacity=".6"/><path d="M420 824h180" stroke-width="11" opacity=".4"/><path d="M470 880h100" stroke-width="9" opacity=".25"/></g>
<g fill="none" stroke="#9cc4dc" stroke-opacity=".28" stroke-width="7" stroke-linecap="round"><path d="M120 760h130"/><path d="M750 800h150"/><path d="M180 880h110"/><path d="M710 910h120"/></g>'''
    fg = '''<defs><filter id="g" x="-80%" y="-80%" width="260%" height="260%"><feGaussianBlur stdDeviation="26"/></filter>
<linearGradient id="h" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fbf3e2"/><stop offset="1" stop-color="#d8ccb6"/></linearGradient></defs>
<path d="M172 420C216 470 250 540 306 606" fill="none" stroke="#f7e8c8" stroke-width="15" stroke-linecap="round"/>
<rect x="124" y="396" width="70" height="320" rx="14" fill="#141a2c"/><rect x="108" y="376" width="102" height="40" rx="16" fill="#2a3350"/>
<path d="M480 300H760L800 590H440Z" fill="url(#h)"/><path d="M452 268H790Q806 268 806 284V300Q806 312 790 312H452Q436 312 436 300V284Q436 268 452 268Z" fill="#e7502c"/>
<rect x="500" y="356" width="240" height="150" rx="16" fill="#ffc768" filter="url(#g)" opacity=".8"/>
<rect x="500" y="356" width="240" height="150" rx="16" fill="#ffc768"/>
<path d="M538 396l42 34-42 34" fill="none" stroke="#3f2a10" stroke-width="20" stroke-linecap="round" stroke-linejoin="round"/><path d="M606 470h82" stroke="#3f2a10" stroke-width="20" stroke-linecap="round"/>
<path d="M240 590H910C886 668 836 716 770 730H390C326 716 268 668 240 590Z" fill="#111828"/><path d="M240 590H910L898 630H254Z" fill="#e7502c"/>'''
    tray = lambda off: '<path fill-rule="evenodd" d="M6 3H13V9H6ZM8 5V7H11V5Z"/><path d="M1 10H17L15 14H3Z"/><rect x="2" y="16" width="14" height="1"/>'
    return bg, fg, tray

def buoy():
    bg = '''<defs><radialGradient id="o" cx=".5" cy=".72" r=".85"><stop offset="0" stop-color="#2fb6b2"/><stop offset=".5" stop-color="#0f7686"/><stop offset="1" stop-color="#083a52"/></radialGradient></defs>
<rect width="1024" height="1024" fill="url(#o)"/>
<g fill="none" stroke="#c9f4ec"><ellipse cx="512" cy="760" rx="240" ry="58" stroke-width="13" stroke-opacity=".5"/><ellipse cx="512" cy="760" rx="350" ry="94" stroke-width="10" stroke-opacity=".28"/><ellipse cx="512" cy="760" rx="470" ry="134" stroke-width="8" stroke-opacity=".14"/></g>'''
    fg = '''<defs><radialGradient id="b" cx=".36" cy=".28" r=".85"><stop offset="0" stop-color="#ff9a6b"/><stop offset=".5" stop-color="#ea4d2a"/><stop offset="1" stop-color="#8d2411"/></radialGradient>
<clipPath id="above"><path d="M0 0H1024V700H762Q512 800 262 700H0Z"/></clipPath>
<clipPath id="ball"><circle cx="512" cy="540" r="258"/></clipPath>
<linearGradient id="d" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#6d1c0d" stop-opacity=".75"/><stop offset="1" stop-color="#6d1c0d" stop-opacity="0"/></linearGradient></defs>
<path d="M290 722Q512 806 734 722Q690 850 512 868Q334 850 290 722Z" fill="url(#d)"/>
<circle cx="512" cy="228" r="56" fill="none" stroke="#39445a" stroke-width="26"/><circle cx="512" cy="228" r="56" fill="none" stroke="#8fa0b4" stroke-width="8" transform="translate(-4 -5)"/>
<rect x="484" y="276" width="56" height="42" rx="8" fill="#39445a"/>
<g clip-path="url(#above)"><circle cx="512" cy="540" r="258" fill="url(#b)"/>
<g clip-path="url(#ball)"><path d="M230 500Q512 590 794 500V580Q512 670 230 580Z" fill="#f7efdf"/></g>
<ellipse cx="420" cy="400" rx="66" ry="38" fill="#fff" opacity=".32" transform="rotate(-28 420 400)"/></g>'''
    tray = lambda off: '<path fill-rule="evenodd" d="M7 0H11V3H10V4H12.5L14 6V8H4V6L5.5 4H8V3H7ZM8 1V2H10V1Z"/><path d="M4 10H14V11L13 13H5L4 11Z"/><rect x="1" y="15" width="16" height="2"/>'
    return bg, fg, tray

for name, fn, alpha in (('rope', rope, False), ('boat', boat, True), ('buoy', buoy, True)):
    bg, fg, tray = fn()
    (out/f'{name}-bg.svg').write_text(SVG % bg); (out/f'{name}-fg.svg').write_text(SVG % fg)
    tsvg = '<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 18 18" fill="currentColor"%s>%s</svg>'
    (out/f'{name}-tray.svg').write_text(tsvg % ('', tray(False)))
    (out/f'{name}-tray-off.svg').write_text(tsvg % (' opacity=".35"' if alpha else '', tray(True)))
