from pathlib import Path
import json
import shutil
root=Path(__file__).parent
# 每个候选由原生矢量形状定义，缩放不会依赖位图纹理。
designs=[
('01','切光','让一道断开的电光成为品牌记忆。','#172734','#66dcf1','#eef9ff', '<path d="M77 25 42 59h23Z M88 65H66l-11 38Z" fill="{fg}"/>', ['断开的闪电轮廓','冷青与深墨底']),
('02','疾行 F','把 Flashcast 的 F 压成前进的楔形。','#e5f1ff','#285ccc','#8bb5ff','<path d="M42 30h55L84 47H62l-5 13h25L68 78H52l-8 22H24Z" fill="{fg}"/><path d="m79 61-9 15h12l-9 20 28-35Z" fill="{accent}"/>',['斜切 F 字母','独立的下行电光']),
('03','快门','将瞬间唤起转译为四片打开的快门。','#171c33','#f3f4ff','#869bff','<path d="m64 24 24 24-24 12-12-12Z M104 64 80 88 68 64l12-12Z M64 104 40 80 64 68l12 12Z M24 64 48 40l12 24-12 12Z" fill="{fg}"/><path d="m64 49 15 15-15 15-15-15Z" fill="{accent}"/>',['四向旋转的实心叶片','中央菱形光点']),
('04','双轨','两道快进轨道指向下一次行动。','#ffefe1','#d45b27','#e99045','<path d="M26 35h20l29 29-29 29H26l29-29Z" fill="{fg}"/><path d="M60 35h19l29 29-29 29H60l29-29Z" fill="{accent}"/>',['双楔形箭头','暖色速度标记']),
('05','投射','从一点把内容投射到目标，呼应 cast。','#142a2c','#b6f4df','#58c8b2','<circle cx="35" cy="64" r="10" fill="{fg}"/><path d="m55 42 45-14v72L55 86Z" fill="{fg}"/><path d="M55 52 100 38v17L55 67Z" fill="{accent}"/>',['圆点与展开光束','非对称构图']),
('06','指令','保留键盘启动器的命令语言，压缩为一个动作。','#1d2431','#e8f0ff','#73ccea','<path d="m27 40 24 24-24 24" fill="none" stroke="{fg}" stroke-width="12" stroke-linejoin="round"/><path d="M69 82h32" stroke="{accent}" stroke-width="12" stroke-linecap="round"/>',['命令提示符与光标','宽笔画，无细节噪音']),
('07','电路环','把连续可用的入口做成一道开放的电路。','#e7edf4','#344e70','#528bcc','<path d="M92 43a36 36 0 1 0 0 42" fill="none" stroke="{fg}" stroke-width="11" stroke-linecap="round"/><path d="m71 30-22 36h17l-6 30 28-43H70Z" fill="{accent}"/>',['断开的圆环','环内小闪电']),
('08','折带','一条折起的带子，形成速度和空间感。','#f0f3f9','#3470d0','#183855','<path d="M29 36h66L74 59H47l27 33H48L22 60Z" fill="{fg}"/><path d="M47 59h27l-9 12Z" fill="{accent}"/>',['连续带状轮廓','单处深色折面']),
('09','负形电光','用整块亮色与切口，在小尺寸上保持冲击力。','#447fce','#f4f8ff','#c8e7ff','<path d="M45 28h49L66 60h24l-49 43 9-33H29Z" fill="{fg}"/><path d="m49 28 7 17" stroke="{bg}" stroke-width="5"/>',['亮色底与白色切口','宽大的闪电负形']),
('10','FC 联结','把 F 和 C 合成一个可独立识别的符号。','#202733','#b4d5fc','#ffffff','<path d="M28 33h29v13H41v12h14v13H41v25H28Z" fill="{fg}"/><path d="M98 36H79a15 15 0 0 0-15 15v29a15 15 0 0 0 15 15h19V81H80V50h18Z" fill="{accent}"/><path d="m85 55 17 10-17 10Z" fill="{fg}"/>',['F 与开放 C 的联字','C 内指向右侧的箭头']),
]
candidates=[]
for num,name,concept,bg,fg,accent,motif,traits in designs:
 shape=motif.format(bg=bg,fg=fg,accent=accent)
 svg=f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><rect x="6" y="6" width="116" height="116" rx="27" fill="{bg}"/>{shape}</svg>'
 (root/f'{num}.svg').write_text(svg)
 # 单色稿用于查看轮廓；使用同一画布的剪切 mask 保留镂空。
 mono=motif.format(bg='black',fg='white',accent='white')
 (root/f'{num}-mono.svg').write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><defs><mask id="shape"><rect width="128" height="128" fill="black"/>{mono}</mask></defs><rect width="128" height="128" fill="currentColor" mask="url(#shape)"/></svg>')
 previews=''.join(f'<div><img src="{num}.svg" width="{s}" height="{s}"/><span>{s}px</span></div>' for s in [16,24,32,48])
 html=f'''<!doctype html><html lang="zh"><head><meta charset="utf-8"><title>{num} {name}</title><style>
:root{{font-family:system-ui,sans-serif;color:#203048;background:#eef1f5}}*{{box-sizing:border-box}}body{{margin:0;padding:42px 56px}}header{{display:flex;justify-content:space-between;align-items:baseline;font-size:14px;color:#526174}}h1{{font-size:30px;margin:0}}.stages{{display:grid;grid-template-columns:1fr 1fr;gap:20px;margin:24px 0}}.stage{{height:420px;display:grid;place-items:center;border-radius:20px;background:#fff}}.dark{{background:#151c27}}.hero{{width:270px;height:270px}}.samples{{display:flex;align-items:end;gap:40px;padding:25px 0}}.samples div{{display:flex;align-items:center;gap:12px;font-size:13px}}.samples span{{color:#657286}}.mono{{margin-left:auto}}p{{font-size:16px;color:#526174;line-height:1.7}}
</style></head><body><header><h1>{num} · {name}</h1><span>FLASHCAST / 图标方向</span></header><p>{concept}</p><div class="stages"><div class="stage"><img class="hero" src="{num}.svg"/></div><div class="stage dark"><img class="hero" src="{num}.svg"/></div></div><div class="samples">{previews}<div class="mono"><img src="{num}-mono.svg" width="24" height="24"/><span>单色轮廓</span></div></div></body></html>'''
 (root/f'{num}.html').write_text(html)
 candidates.append(dict(id=f'icon-{num}',name=name,concept=concept,typography='系统无衬线；标题 30px，说明 16px',palette=[bg,fg,accent],traits=traits,kind='html',source=f'{num}.html'))
shutil.copyfile(root.parents[2]/'src-tauri/icons/128x128@2x.png',root/'current.png')
current=(root/'01.html').read_text().replace('01 · 切光','现状 · 当前图标').replace('01 切光','当前图标').replace('让一道断开的电光成为品牌记忆。','当前应用使用的深色底黄色闪电。').replace('src="01.svg"','src="current.png"').replace('<div class="mono"><img src="01-mono.svg" width="24" height="24"/><span>单色轮廓</span></div>','')
(root/'current.html').write_text(current)
candidates.insert(0,dict(id='current',name='当前图标',concept='保留当前图标作为基线，比较轮廓和小尺寸辨识。',typography='系统无衬线；标题30px、说明16px',palette=['#181b24','#ffc94b'],traits=['旧版黄色闪电','尖角触及背景边缘'],kind='html',source='current.html',baseline=True))
manifest=dict(schemaVersion=1,project='Flashcast · 10 版应用图标',brief='简洁、精致、契合闪电与快速投射；同时比较浅深背景和小尺寸识别。',round='图标 01',candidates=candidates)
(root/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2))
# 联系表是一张独立矢量画板，方便一眼比较十个轮廓。
parts=['<svg xmlns="http://www.w3.org/2000/svg" width="1400" height="820" viewBox="0 0 1400 820"><rect width="1400" height="820" fill="#eef1f5"/><text x="48" y="62" font-family="sans-serif" font-weight="600" font-size="28" fill="#253448">FLASHCAST · 10 个图标方向</text><text x="48" y="96" font-family="sans-serif" font-size="16" fill="#657286">同尺寸比较形状 · 原版、32px 与单色轮廓</text>']
for index,(num,name,concept,bg,fg,accent,motif,traits) in enumerate(designs):
 x=40+(index%5)*266;y=130+(index//5)*327
 shape=motif.format(bg=bg,fg=fg,accent=accent)
 parts.append(f'<g transform="translate({x} {y})"><rect width="252" height="307" rx="16" fill="#ffffff"/><svg x="50" y="18" width="152" height="152" viewBox="0 0 128 128"><rect x="6" y="6" width="116" height="116" rx="27" fill="{bg}"/>{shape}</svg><text x="20" y="205" font-family="sans-serif" font-size="18" font-weight="600" fill="#253448">{num} · {name}</text><svg x="20" y="234" width="32" height="32" viewBox="0 0 128 128"><rect x="6" y="6" width="116" height="116" rx="27" fill="{bg}"/>{shape}</svg><text x="64" y="256" font-family="sans-serif" font-size="13" fill="#657286">32px</text><svg x="166" y="236" width="24" height="24" viewBox="0 0 128 128">{motif.format(bg="#fff",fg="#253448",accent="#253448")}</svg><text x="198" y="255" font-family="sans-serif" font-size="13" fill="#657286">单色</text></g>')
parts.append('</svg>')
(root/'board.svg').write_text(''.join(parts))
