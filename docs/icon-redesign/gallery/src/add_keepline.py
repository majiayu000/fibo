#!/usr/bin/env python3
"""Adds the Keepline-era drafts to the gallery. Usage: add_keepline.py <dir holding docs/assets from git archive>"""
import json,pathlib,re,subprocess,sys
A=pathlib.Path(sys.argv[1]);G=pathlib.Path(__file__).resolve().parent.parent
page=(G/'index.html').read_text()
data=json.loads(re.search(r'<script id="data" type="application/json">(.*?)</script>',page,re.S).group(1).replace('<\\/','</'))
data['batches']['0']='第 0 轮：Keepline 时期'
data['items']=[i for i in data['items'] if i['batch']!=0]
L=[('水墨圆环：你现在用的这条线','v8b-clean','水墨圆环·v8b（现在的定稿）','炭黑渐淡到灰，笔画稍粗；缺口里一块朱红光标。','32 px 下最清楚，在 Dock 里最有存在感。后来成了现在的图标。'),
('水墨圆环：你现在用的这条线','v8a-clean','水墨圆环·v8a','中灰淡墨，笔画较细，更空灵。','32 px 下圆环偏淡。'),
('水墨圆环：你现在用的这条线','v7c','水墨·v7c','宣纸白底，淡墨一笔画出的圆，朱红光标像一枚印章。','最有个性、最有东方味；边缘有少量噪点，纸纹在小尺寸下会变脏。'),
('水墨圆环：你现在用的这条线','v7b','月夜·v7b','深靛蓝底，淡银紫色细圆环，琥珀色光标。','当时认为空灵和辨识度平衡得最好，32 px 清楚，边缘干净。'),
('水墨圆环：你现在用的这条线','v7a','晨雾·v7a','珍珠白到淡蓝紫的渐变，磨砂玻璃圆环。','最空灵，但对比太弱，32 px 下圆环几乎看不见。'),
('水墨圆环：你现在用的这条线','v6d','补全的圆环·v6d','深紫底，淡紫圆环，橙色光标在缺口处，把断掉的工作补上。这是这条线的起点。','最简洁、品牌感最强；“补全”的动作感偏弱，乍看像加载圈。'),
('墨团精灵：现在的吉祥物这条线','m2a-clean','墨团精灵·日出（定稿）','圆环从珊瑚红到浅金，墨团坐在环里，把光标放进缺口。','表情最生动；全是暖色，光标不够跳。后来成了现在的吉祥物图标。'),
('墨团精灵：现在的吉祥物这条线','m2b-clean','墨团精灵·糖果天空','圆环从水青到紫色，淡紫薄荷底。','冷色里朱红光标最醒目，更像科技产品；卷须不明显。'),
('墨团精灵：现在的吉祥物这条线','m1d-clean','墨团精灵·第一版','坐在彩色圆环里，把光标放进缺口，和水墨图标联系最紧。','当时认为最独特、不像常见的 AI 动物图。'),
('其他吉祥物','m1a-clean','松鼠管家','把光标当松果抱在怀里，“替你收好”。','光标有点像手机；松鼠是常见吉祥物题材。'),
('其他吉祥物','m1b-clean','夜猫头鹰','轮廓清楚，替你守夜。','光标拿在手里像在看手机。'),
('其他吉祥物','m1c-clean','叼回来的柴犬','把丢掉的工作叼回来，最有活力。','嘴里的光标像一块糖。'),
('第二批方向','v6a','灯塔·v6a','夜蓝底，暖色光束末端是光标。','意思清楚、做工完整；灯塔较常见，细节偏多。'),
('第二批方向','v6b','绳结·v6b','米色底，棕色绳结。','看起来像香肠，看不出和终端有关。'),
('第二批方向','v6c','船锚提示符·v6c','深青底，> 和锚拼在一起。','拼得生硬，像数字 7 或 2。'),
('第一批方向','v5b','终端书签·v5b','金色书签加光标，紫色底，“进度已保存”。','最好懂；发光偏重，> 不明显。'),
('第一批方向','v5a','缝合的线·v5a','红线配米白底，把断线缝回去。','缝针像“≠”或铁丝网。'),
('第一批方向','v5c','接力·v5c','蓝色加红点，浅色底。','像常见的“断开链接”图标。'),
('第一批方向','v5d','被守护的线·v5d','荧光绿配黑底。','像针管或温度计。'),
('最早的折线','v5','折线·v5','深绿底，暖白折线和独立的铜橙色光标。','偏向 > 的做法。'),
('最早的折线','v4','两个窗格·v4','两个错位的终端窗格，一条线穿过去。','32 px 下线几乎看不见。'),
('最早的折线','v3','K 形折带·v3','K 形轮廓，两段折带交接。','K 字母方向当时被用户否定。'),
('最早的折线','v2','连续折带·v2','深绿底，暖白连续折带，铜橙色光标。','右侧弯钩像回车键。')]
for g,f,n,idea,risk in L:
    src=A/(('keepline-mascot-' if f.startswith('m') else 'keepline-icon-')+f+'.png')
    id='0-'+f;subprocess.run(['sips','-Z','256',str(src),'--out',str(G/'img'/f'{id}.png')],check=True,capture_output=True)
    data['items'].append(dict(id=id,batch=0,group=g,name=n,idea=idea,risk=risk,img=f'img/{id}.png',on='',off='',need='',status='',anim=False,by='Codex'))
(G/'index.html').write_text(page.replace(re.search(r'<script id="data" type="application/json">(.*?)</script>',page,re.S).group(1),json.dumps(data,ensure_ascii=False).replace('</','<\\/')))
print(len(data['items']),'items')
