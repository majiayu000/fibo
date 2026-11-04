#!/usr/bin/env python3
"""Collects every icon draft into one gallery page: gallery/index.html plus gallery/img/. Usage: build.py <scratch dir>"""
import json,pathlib,re,subprocess,sys,html
S=pathlib.Path(sys.argv[1]);G=pathlib.Path(__file__).resolve().parent.parent;R=G.parent;IMG=G/'img'
Items=[]
def thumb(src,id):
    if not (IMG/f'{id}.png').exists():
        subprocess.run(['sips','-Z','256',str(src),'--out',str(IMG/f'{id}.png')],check=True,capture_output=True)
    return f'img/{id}.png'
def rd(p): return p.read_text() if p.exists() else ''
def add(batch,group,k,name,idea,risk,src,on='',off='',need='',status='',anim=False,by='Claude'):
    id=f'{batch}-{k}';Items.append(dict(id=id,batch=batch,group=group,name=name,idea=idea,risk=risk.strip(),img=thumb(src,id),on=on,off=off,need=need or on,status=status,anim=anim,by=by))
slice_=lambda t,a,b:t[t.index(a):t.index(b)]
# Rounds 10-12
v=S/'v10';notes={'boomA':'细化过头：瘦得像弯刀或鹰嘴，不如第 9 轮结实。','boomB':'镖太瘦，这圈虚线倒是好看。','boomC':'镖太瘦，像一根象牙。','keyA':'实心翼长得像发芽，不像钥匙了。','keyB':'实心翼长得像发芽，不像钥匙了。','crab':'画出来像一架雪橇。','snap':'画出来看不出是手。','dog':'画出来太生硬，看不出是狗。','sew':'画出来像字母 T。','wheel':'画出来更像船舵或雪花。','drum':'画出来也有点像相机。','boatPix':'单色之后像吃豆人。','boatGrad':'单色之后像饼图。','boatGlass':'单色之后像饼图。','boatLine':'单色之后像饼图。','boatNeon':'单色之后像饼图。','boomNeon':'两头能看到接缝，这版没画好。','boomLine':'两头能看到接缝，这版没画好。','kiteNeon':'线和风筝接头处有接缝，这版没画好。','kiteLine':'线和风筝接头处有接缝，这版没画好。','boomClay':'立体之后发糊，比平面版差。','kiteClay':'立体之后发糊，比平面版差。','boatClay':'立体之后发糊，比平面版差。'}
fails10=set(notes)-{'boomB','drum'};recs10={'boomD','kiteA','kiteB','kiteC','boatA','boatB','stretch','wrench','wood','dial','kitePix','kitePaper','boomPaper'}
for e in json.load(open(v/'svg/meta.json')):
    n=e['k'];add(e['round'],e['group'].replace('（Codex）',''),n,e['name'],e['idea'],e['risk']+notes.get(n,''),v/f'render/{n}-512.png',rd(v/f'svg/{n}-tray.svg'),rd(v/f'svg/{n}-off.svg'),status='fail' if n in fails10 else 'rec' if n in recs10 else '',by=e['by'])
# Batch 9
v=S/'v9';fails={'key':'画出来像一把剪刀。','boom2':'两色之后像巨嘴鸟的嘴。','p7':'画出来很像吃豆人的幽灵，也和 Ghostty 撞了。','p8':'和方块边那张一样像幽灵。','p9':'和方块边那张一样像幽灵。','top':'画出来更像一片叶子。','yoyo':'画出来像唱片或拉绳开关。','br1':'放进 Dock 太克制，灯几乎看不见。','g4':'小尺寸只剩一个圆，也像按钮。'}
recs={'boom','k1','k2','br3','b3','s5','g2','g1','p6'}
for e in json.load(open(v/'svg/meta.json')):
    n=e['k'];add(9,e['group'],n,e['name'],e['idea'],e['risk']+fails.get(n,''),v/f'render/{n}-512.png',rd(v/f'svg/{n}-tray.svg'),rd(v/f'svg/{n}-off.svg'),status='fail' if n in fails else 'rec' if n in recs else '',anim=e.get('anim',False))
# Batch 8
v=S/'v8';fails={'sprout':'画出来更像一盏台灯。','pop':'画出来更像冰淇淋。','card':'画出来更像 SIM 卡。','stars':'画出来更像股票走势图。','fire':'画出来像天使光环，不像虫子。','egg':'切口像戴了顶帽子。','pulse':'和健康类 App 太接近。'}
recs={'kite','breath','glass','p4','s3','b2','kite2'}
for e in json.load(open(v/'svg/meta.json')):
    n=e['k'];add(8,e['group'],n,e['name'],e['idea'],e['risk']+fails.get(n,''),v/f'render/{n}-512.png',rd(v/f'svg/{n}-tray.svg'),rd(v/f'svg/{n}-off.svg'),status='fail' if n in fails else 'rec' if n in recs else '',by=e['by'],anim=n=='breath')
# Batch 7
v=S/'v7';fails={'chair':'渲染出来还很像字母 h。','reef':'画出来更像字母 S，这版没成。','fit':'画出来更像文件夹。','ripple':''};recs={'peek','sails','toast','mail','bau','tug'}
for e in json.load(open(v/'svg/meta.json')):
    n=e['k'];add(7,e['group'],n,e['name'],e['idea'],e['risk']+fails.get(n,''),v/f'render/{n}-512.png',rd(v/f'svg/{n}-tray.svg'),rd(v/f'svg/{n}-off.svg'),rd(v/f'svg/{n}-need.svg'),status='fail' if n in fails else 'rec' if n in recs else '',by=e['by'])
# Batch 6
v=S/'v6';recs={'bleed','split','taper','round'};bad={'flat':'这版头尾有破口，没画好。','heavy':'这版内弯有折痕，没画好。'}
for n,(nm,note) in json.load(open(v/'svg/meta.json')).items():
    add(6,'信号黄上的波浪号',n,nm,note.replace('（头尾还有破口，这版没画好。）','').replace('（内弯处有折痕，这版没画好。）',''),bad.get(n,''),v/f'render/{n}-512.png',rd(v/f'svg/{n}-tray.svg'),rd(v/f'svg/{n}-tray-off.svg'),status='fail' if n in bad else 'rec' if n in recs else '')
on,off=rd(v/'svg/base-tray.svg'),rd(v/'svg/base-tray-off.svg')
for c,nm,g,note in [('y-marigold','金盏','黄色深浅','更暖更亮，但太接近 Claude 的橙色。'),('y-lemon','柠檬','黄色深浅','最亮，也最容易有警示感。'),('y-mustard','芥末','黄色深浅','发脏，不够精神。'),('y-butter','奶油','黄色深浅','太弱，放进一排里不跳。'),('c-navy','配海军蓝','黄底配什么颜色','比墨色软一点。'),('c-green','配深绿','黄底配什么颜色','对比够，气质更自然。'),('c-red','配朱红','黄底配什么颜色','黄底红字对比不够。'),('c-cream','配米白','黄底配什么颜色','对比太弱。'),('c-invert','墨底黄字','黄底配什么颜色','回到了最挤的黑底那一堆。')]:
    add(6,g,c,nm,note,'',v/f'render/{c}-128.png',on,off)
# Batch 5
v=S/'v5';m=json.load(open(v/'svg/meta.json'));cost={'tilde':'单独一个 ~ 很难独占，要靠颜色和笔形来记；WaveTerm 也用了波浪形。','knot':'容易被读成无穷符号或 8；托盘最挤。','m':'字母图标很多，故事最少。','hitch':'可能看成螺栓或柱子；托盘一倍屏糊成一团。'}
for n,(nm,why) in m.items():
    for c,lb in (('green','港湾绿'),('yellow','信号黄'),('paper','纸白')):
        src=v/f'render/{n}-{c}-512.png' if c=='green' else v/f'render/{n}-{c}-128.png'
        add(5,'一个形状，一个底色',f'{n}-{c}',f'{nm}·{lb}',why,cost[n],src,rd(v/f'svg/{n}-tray.svg'),rd(v/f'svg/{n}-tray-off.svg'),status='rec' if (n,c)==('tilde','yellow') else 'fail' if n=='hitch' else '')
# Batch 4: rough
v=S/'v4'
for n,(nm,why) in json.load(open(v/'svg/meta.json')).items():
    add(4,'第一次照 Claude 的做法（废稿）',n,nm,why,'随机抖动出来的粗糙多边形，这批当时就废了，没给你看过。',v/f'render/{n}-green-512.png',rd(v/f'svg/{n}-tray.svg'),rd(v/f'svg/{n}-tray-off.svg'),status='fail')
# Batch 3
v=S/'v3';ns={};exec(slice_(rd(v/'page.py'),'C=[',"Peers="),ns)
for n,t,idea,good,bad in ns['C']:
    add(3,'往同类的空白处发散',n,t,idea+good,bad,v/f'render/{n}-512.png',rd(v/f'svg/{n}-tray.svg'),rd(v/f'svg/{n}-tray-off.svg'),rd(v/f'svg/{n}-tray-need.svg'))
# Batch 2
r=R/'v2/render';ns={};exec(slice_(rd(R/'v2/src/page.py'),'D = [','tray ='),ns)
for n,t,why,cost in ns['D']:
    add(2,'第一次从头重画',n,t.split('　')[-1],why,cost,r/f'{n}-1024.png',rd(r/f'{n}-tray.svg'),rd(r/f'{n}-tray-off.svg'))
# Batch 1
v=S/'v1r';T=lambda b,o='':f'<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 18 18" fill="currentColor"{o}>{b}</svg>'
add(1,'最早的三个方向和 Codex 的两个','current','现在的图标','现在的水墨缺口环。系统会把它缩小，再套一圈浅色底板。','托盘是文字 K，改名后对不上。',S/'sys-current.png')
ed=lambda dot:'<path d="M2 2H9L16 9L9 16H2V14H8L13 9L8 4H2Z"/>'+dot
add(1,'最早的三个方向和 Codex 的两个','F','折角绕桩','和 Codex 商量后的结果：折角的 > 加方桩。','后来发现 > 是同类里最挤的做法，收回推荐。',v/'render/F-512.png',T(ed('<rect x="7" y="8" width="2" height="2"/>')),T('<path d="M2 2H9L15 8H12L8 4H2ZM12 10H15L9 16H2V14H8Z"/>'))
add(1,'最早的三个方向和 Codex 的两个','A','绕桩','绳眼套在桩上，俯视是一个 >。','像一只眼睛；断线不够醒目。',v/'render/A-512.png',T('<path d="M2.4 2.4L12.5 4.7A4.4 4.4 0 0 1 12.5 13.3L2.4 15.6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/><circle cx="11.5" cy="9" r="1.75"/>'),T('<path d="M2.4 2.4L12.5 4.7A4.4 4.4 0 0 1 12.5 13.3L2.4 15.6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>'))
add(1,'最早的三个方向和 Codex 的两个','C','一笔 M','一笔写成 M，右脚落在浮标上。','脚下的点读起来像感叹号，平时也像在报警。',v/'render/C-512.png',T('<path d="M2.6 15.2V3L8.2 9.6L13.8 3V9.6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/><circle cx="13.8" cy="13.6" r="1.9"/>'),T('<path d="M2.6 15.2V3L8.2 9.6L13.8 3V9.6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>'),status='fail')
add(1,'最早的三个方向和 Codex 的两个','D','系缆桩','Codex 的第一个方向：一个立住的桩。','渲染出来像工字钢和文本光标。',v/'render/D-512.png',T('<path d="M2 4H16V6H13L10 9V13H14V15H4V13H8V9L5 6H2Z"/>'),T('<path d="M2 4H16V6H13L10 9V10H8V9L5 6H2Z"/><path d="M4 13H14V15H4Z"/>'),status='fail',by='Codex')
add(1,'最早的三个方向和 Codex 的两个','E','双桩横缆','Codex 的第二个方向：两根桩一根横缆。','读起来像字母 HT。',v/'render/E-512.png',T('<path d="M1 3H8V5H6V10H12V8H10V6H17V8H15V15H12V12H6V15H3V5H1Z"/>'),T('<path d="M1 3H8V5H6V15H3V5H1Z"/><path d="M10 6H17V8H15V15H12V8H10Z"/>'),status='fail',by='Codex')
Batches={12:'第 12 轮：换画法',11:'第 11 轮：新点子',10:'第 10 轮：细化',9:'第 9 轮：继续发展',8:'第 8 轮：创意墙二',7:'第 7 轮：创意墙一',6:'第 6 轮：信号黄',5:'第 5 轮：一个形状',4:'第 4 轮：废稿',3:'第 3 轮：七个发散',2:'第 2 轮：重画',1:'第 1 轮：最早'}
data=json.dumps(dict(items=Items,batches=Batches),ensure_ascii=False).replace('</','<\\/')
tpl=(G/'src/template.html').read_text()
(G/'index.html').write_text(tpl.replace('__DATA__',data))
print(len(Items),'items')
