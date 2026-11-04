#!/usr/bin/env python3
"""Writes v2/index.html next to render/, inlining the tray SVGs so they can follow the bar's colour."""
import pathlib, json
root = pathlib.Path(__file__).resolve().parent.parent; r = root/'render'
D = [('rope','甲　绳眼','一根真的缆绳套在朱红的桩上，绳子跑出画面，像绷紧着。形状本身还是一个 >，所以同时说了“系泊”和“终端”。托盘：绳眼加方桩，连不上后台时桩消失。','最抽象，要靠名字才知道是“系泊”；绳纹在 32pt 以下会糊成一条米色带；托盘沿用了上一轮 F 的折角形状，只放大了一点，不是全新的。'),
('boat','乙　夜泊','傍晚的小船系在桩上，驾驶舱亮着 >_。讲的正是这个产品：你走开了，船还系在那里，灯还亮着，里面还在跑。托盘：一条小船，连不上后台时整体变淡。','最有故事，但最童话，和其他开发工具放一起风格最活泼；托盘小到 18pt 只剩方块加船底，更像印章，这一版还没画好。'),
('buoy','丙　浮标','水面上一个带系缆环的红色系泊浮标。三个里颜色最跳，在 Dock 里最好找。托盘：环、球和水线，连不上后台时整体变淡。','和“终端”没有关系；大图可能先被看成圣诞球或鱼漂；托盘像叠起来的瓶子，这一版也没画好。')]
tray = {n:(( r/f'{n}-tray.svg').read_text(), (r/f'{n}-tray-off.svg').read_text()) for n,*_ in D}
wifi='<svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M1.5 6.2a9.5 9.5 0 0 1 13 0M3.9 8.8a6 6 0 0 1 8.2 0M6.2 11.2a2.7 2.7 0 0 1 3.6 0"/></svg>'
batt='<svg width="25" height="13" viewBox="0 0 25 13"><rect x=".5" y=".5" width="21" height="12" rx="3.2" fill="none" stroke="currentColor" opacity=".5"/><rect x="2" y="2" width="15" height="9" rx="1.8" fill="currentColor"/><rect x="23" y="4.5" width="1.6" height="4" rx=".8" fill="currentColor" opacity=".5"/></svg>'
bar=lambda t,m:f'<div class="bar {t}"><span class="tray">{m}</span>{wifi}{batt}<span>周三 21:40</span></div>'
sec=''
for n,title,why,cost in D:
    on,off=tray[n]
    rows=''.join(bar(t,m) for m in (on,on+'<span>3</span>',off) for t in ('light','dark'))
    zoom=''.join(f'<figure><canvas data-svg="{json.dumps(s).replace(chr(34),"&quot;")}" data-bg="{b}" data-fg="{f}"></canvas><figcaption>{c}</figcaption></figure>' for s,b,f,c in ((on,'#ececee','#111','平时，一倍屏放大 6 倍'),(on,'#2b2d33','#fff','深色菜单栏'),(off,'#ececee','#111','连不上后台')))
    sizes=''.join(f'<figure><img src="render/{n}-{s}.png" width="{w}" height="{w}" alt=""><figcaption>{w}</figcaption></figure>' for s,w in ((1024,128),(128,64),(64,32),(32,16)))
    sec+=f'''<section><h2>{title}</h2><p class="why">{why}</p><p class="cost"><b>不足：</b>{cost}</p>
<div class="grid"><img class="hero" src="render/{n}-1024.png" alt="{title}"><div><p class="label">系统渲染，真实尺寸（pt）</p><div class="sizes">{sizes}</div>
<p class="label">菜单栏：平时 / 3 个要你处理 / 连不上后台</p><div class="bars">{rows}</div><div class="zoom">{zoom}</div></div></div></section>'''
strip=''.join(f'<figure><img src="render/{n}-128.png" width="64" height="64" alt=""><figcaption>{c}</figcaption></figure>' for n,c in (('current','现在'),('rope','甲'),('boat','乙'),('buoy','丙')))
(root/'index.html').write_text(f'''<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Mooring 图标重做</title>
<style>
:root{{--bg:#f6f5f2;--fg:#1d2026;--muted:#6b6f78;--line:#dddbd5;--card:#fff}}
@media (prefers-color-scheme:dark){{:root{{--bg:#191c21;--fg:#e9e8e4;--muted:#979ba4;--line:#30343b;--card:#21252c}}}}
*{{box-sizing:border-box}}body{{margin:0;background:var(--bg);color:var(--fg);font:15px/1.6 -apple-system,"PingFang SC",sans-serif}}
main{{max-width:1080px;margin:auto;padding:40px 24px 80px}}h1{{font-size:26px;margin:0 0 6px}}h2{{font-size:20px;margin:0}}p{{margin:0}}
.lead,.cost,.why{{max-width:72ch}}.lead,.cost{{color:var(--muted)}}.why{{margin-top:6px}}.cost{{margin-top:4px}}
section,.strip{{margin-top:24px;padding:24px;background:var(--card);border:1px solid var(--line);border-radius:12px}}
.strip{{display:flex;gap:24px;flex-wrap:wrap;background:linear-gradient(120deg,#31506e,#a77a8a 60%,#e0a878);border:0}}.strip figcaption{{color:#fff}}
figure{{margin:0;text-align:center}}figcaption{{font-size:11px;color:var(--muted);margin-top:4px}}
.grid{{display:grid;grid-template-columns:300px 1fr;gap:28px;margin-top:16px}}.hero{{width:300px;height:300px;display:block}}
.label{{font-size:12px;color:var(--muted);margin-bottom:8px}}.sizes{{display:flex;align-items:flex-end;gap:16px;margin-bottom:20px;flex-wrap:wrap}}.sizes img{{display:block;margin:auto}}
.bars{{display:grid;grid-template-columns:1fr 1fr;gap:8px}}.bar{{height:24px;border-radius:5px;display:flex;align-items:center;justify-content:flex-end;gap:13px;padding:0 10px;font-size:13px;font-weight:500;white-space:nowrap;overflow:hidden}}
.bar.light{{background:#ececee;color:#111}}.bar.dark{{background:#2b2d33;color:#fff}}.bar svg{{display:block;flex:none}}.tray{{display:flex;align-items:center;gap:3px}}
.zoom{{display:flex;gap:16px;flex-wrap:wrap;margin-top:18px}}.zoom canvas{{width:108px;height:108px;image-rendering:pixelated;display:block;border-radius:4px;border:1px solid var(--line)}}
@media (max-width:720px){{main{{padding:24px 16px 60px}}section{{padding:16px}}.grid{{grid-template-columns:1fr}}.hero{{width:220px;height:220px}}.bars{{grid-template-columns:1fr}}}}
</style></head><body><main><h1>Mooring 图标重做</h1>
<p class="lead">从头重画的三个方向，全部是 macOS 自己渲染出来的样子。这是给你看的稿子，应用里什么都没换。喜欢哪个、哪里不对，直接说就行。</p>
<div class="strip">{strip}</div>{sec}</main>
<script>document.querySelectorAll('canvas').forEach(c=>{{c.width=c.height=18;
const s=JSON.parse(c.dataset.svg).replaceAll('currentColor',c.dataset.fg),i=new Image();
i.onload=()=>{{const x=c.getContext('2d');x.fillStyle=c.dataset.bg;x.fillRect(0,0,18,18);x.drawImage(i,0,0,18,18)}};
i.src='data:image/svg+xml;charset=utf-8,'+encodeURIComponent(s)}});</script></body></html>
''')
