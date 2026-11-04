#!/usr/bin/env python3
"""Re-applies template.html to the data already in index.html, so page changes need no scratch renders. Usage: refresh.py"""
import json,math,pathlib,re
G=pathlib.Path(__file__).resolve().parent.parent
page=(G/'index.html').read_text()
data=json.loads(re.search(r'<script id="data" type="application/json">(.*?)</script>',page,re.S).group(1).replace('<\\/','</'))
p=lambda a:(9+6.6*math.cos(math.radians(a)),9+6.6*math.sin(math.radians(a)))
A,B,M=p(-6),p(-86),p(-46)
ring=f'<path d="M{A[0]:.2f} {A[1]:.2f}A6.6 6.6 0 1 1 {B[0]:.2f} {B[1]:.2f}" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>'
seal=f'<rect x="{M[0]-1.6:.2f}" y="{M[1]-1.1:.2f}" width="3.2" height="2.2" rx=".5" transform="rotate(-46 {M[0]:.2f} {M[1]:.2f})"/>'
T=lambda b:f'<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 18 18" fill="currentColor">{b}</svg>'
for i in data['items']:
    if i['id']=='1-current':
        i.update(on=T(ring+seal),need=T(ring+seal),off=T(ring),idea='现在的水墨缺口环。系统会把它缩小，再套一圈浅色底板。这里的托盘是第 1 轮“留墨环”的单色版：一个 C 形环加一个小块，连不上后台时小块消失。',risk='现在实际的托盘是文字 K，改名后对不上。这个托盘版当时被 Claude 否掉，因为像“加载中”的转圈，不是用户否的。')
body=json.dumps(data,ensure_ascii=False).replace('</','<\\/')
(G/'index.html').write_text((G/'src/template.html').read_text().replace('__DATA__',body))
print(len(data['items']),'items')
