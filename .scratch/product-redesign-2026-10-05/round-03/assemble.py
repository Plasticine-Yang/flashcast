"""Assemble self-contained, matching Graphite color/material candidates."""
from pathlib import Path
import json
import re

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT.parent / "graphite.html"
PALETTES = {
    "volt": ("雷光", "Volt", ["#1d2023", "#e1ec9c", "#79610b"], "石墨与硫黄闪光；品牌感最鲜明，浅色使用深金保证文字清楚。"),
    "arc": ("电弧", "Arc", ["#19212b", "#a2c4ff", "#265b9d"], "冷石墨与冰蓝电弧；更冷静，适合长时间工作的效率工具。"),
    "silver": ("银闪", "Silver", ["#242322", "#ddc8a6", "#756044"], "中性银灰与少量香槟闪光；表面最安静，颜色集中在选择与动作。"),
}

COMMON = r"""
body{position:relative;min-height:100dvh;padding:112px 24px 42px;background:#7e8a91;color:var(--text);isolation:isolate}
.window{z-index:2;flex:none;position:relative;color:var(--text)}
.desktop{position:fixed;inset:0;z-index:-1;overflow:hidden;pointer-events:none;background:#88969b}
.desktop::before{content:"";position:absolute;inset:0;background:linear-gradient(132deg,#aeb9bc 0 23%,#788991 23% 39%,#566c79 39% 55%,#899b99 55% 77%,#bec5c1 77%)}
.desktop::after{content:"";position:absolute;inset:0;background:linear-gradient(180deg,#ffffff24,transparent 40%,#0d182830)}
body[data-scene="bright"] .desktop::before{background:linear-gradient(132deg,#fafaf5 0 23%,#d0dbe0 23% 39%,#aabcc6 39% 55%,#d9e2dc 55% 77%,#e9e4dc 77%)}
body[data-scene="dark"] .desktop::before{background:linear-gradient(132deg,#303a43 0 23%,#18222e 23% 39%,#1a3443 39% 55%,#263e3f 55% 77%,#44544f 77%)}
body[data-scene="color"] .desktop::before{background:radial-gradient(ellipse at 14% 8%,#f6d4a9,transparent 47%),radial-gradient(ellipse at 83% 12%,#b3cde5,transparent 40%),radial-gradient(ellipse at 82% 84%,#ea9784,transparent 47%),linear-gradient(135deg,#8aa59a,#536a80 52%,#b095a8)}
.desktop .desktop-line{position:absolute;left:-15%;top:50%;width:130%;height:14px;background:#e9eee94a;transform:rotate(-33deg);box-shadow:0 22px 0 #1e334328,0 -16px 0 #ffffff16}
.desk-pane{display:none;position:absolute;left:calc(50% - 430px);top:calc(50% - 180px);width:750px;height:380px;background:#fafafa;border:1px solid #fff9;border-radius:10px;color:#263342;padding:28px;box-shadow:0 20px 60px #14223225;font:14px/2.2 system-ui}
body[data-scene="desk"] .desktop::before{background:#aebcc4}
body[data-scene="desk"] .desk-pane{display:block}
.desk-pane .pane-head{font-weight:600;border-bottom:1px solid #d4d9df;padding-bottom:8px;margin-bottom:12px}
.desk-pane p{display:flex;justify-content:space-between;border-bottom:1px solid #d4d9df;color:#62717f}
.desk-pane strong{color:#263342;font-weight:500}
.comparison-controls{position:absolute;top:28px;left:50%;transform:translateX(-50%);width:min(900px,calc(100% - 48px));z-index:4;display:flex;flex-wrap:wrap;justify-content:space-between;align-items:center;gap:10px;color:#f8fafb;text-shadow:0 1px 8px #13233155;font-size:12px}
.comparison-title{display:flex;gap:10px;align-items:center;font-size:13px;font-weight:600}
.comparison-title small{font-size:12px;color:#eff4f7c9;font-weight:400}
.comparison-controls .controls{display:flex;gap:8px;flex-wrap:wrap;align-items:center}
.comparison-controls .segmented{display:flex;padding:3px;border-radius:8px;background:#18233170;border:1px solid #ffffff36;backdrop-filter:blur(12px);text-shadow:none;gap:2px}
.comparison-controls .segmented button{font-size:12px;padding:5px 9px;color:#edf2f6;border-radius:5px}
.comparison-controls .segmented button.active{background:#f3f5f9;color:#22303e;box-shadow:0 1px 4px #08172530}
.comparison-controls select{background:#182331b3;border:1px solid #ffffff40;border-radius:7px;color:#f5f7fa;font:12px system-ui;padding:6px 9px;text-shadow:none;max-width:130px}
.comparison-controls .solid-check{display:flex;gap:5px;align-items:center;background:#18233170;padding:5px 7px;border:1px solid #ffffff36;border-radius:7px;text-shadow:none}
.comparison-controls input{accent-color:#dbe7f4;margin:0}
.primary{color:var(--accent-ink)!important}.secondary{background:var(--surface)}
.row.selected .kind,.row.selected .enter{color:var(--accent)}
.row.selected::before{width:3px;height:20px;background:var(--accent)}
.flash svg{fill:var(--accent);stroke:var(--accent);width:15px;height:15px}
.flash{display:flex;align-items:center}.brand{letter-spacing:1.25px;font-size:11px;font-weight:650}
.footer .brand{color:var(--muted)}
.row .ico.app{background:var(--surface);color:var(--muted)}
.row.selected .ico{background:color-mix(in srgb,var(--accent) 12%,transparent);color:var(--accent)}
.row .enter{font-size:14px}.search>svg{width:18px;height:18px;opacity:.8}
.search-input{color:var(--text)}
.scopebar button.active{font-weight:600}
.editor input,.editor textarea{color:var(--text)}
.detail.visible{background:var(--surface)}
.nav{background:var(--surface)}
.toast{z-index:4;background:var(--bg);color:var(--text)}
.theme.chosen .thumb{outline-color:var(--accent)}
.theme.chosen .check{color:var(--accent)}
body[data-solid="true"] .window{background:var(--bg)!important;backdrop-filter:none!important;-webkit-backdrop-filter:none!important}
body[data-solid="true"] .window::before,body[data-solid="true"] .window::after{display:none!important}
@supports not (backdrop-filter:blur(1px)){.window{background:var(--bg)!important}}
@media(prefers-reduced-transparency:reduce){.window{background:var(--bg)!important;backdrop-filter:none!important}.window::before,.window::after{display:none!important}}
@media(max-width:700px){body{padding:115px 0 28px}.window{border-radius:16px}.comparison-controls{top:19px;width:calc(100% - 24px);justify-content:center;gap:8px}.comparison-title{width:100%;justify-content:center}.comparison-controls .controls{justify-content:center}.comparison-controls .solid-check{display:none}}
"""

DESKTOP = """<div class="desktop" aria-hidden="true"><div class="desktop-line"></div><div class="desk-pane"><div class="pane-head">工作区 / 文档</div><p><strong>会议邀请.md</strong><span>工作 · 会议</span></p><p><strong>常用回复.md</strong><span>工作 · 回复</span></p><p><strong>settings.toml</strong><span>配置</span></p><p><strong>plugin-manifest.json</strong><span>功能插件</span></p><p><strong>theme.json</strong><span>外观</span></p><p><strong>工作记录.md</strong><span>今天</span></p></div></div>"""

CONTROLS = """<header class="comparison-controls" aria-label="设计比较"><div class="comparison-title">TITLE <small>石墨布局 · 深浅成对</small></div><div class="controls"><div class="segmented" aria-label="外观比较"><button :class="{active:renderedAppearance==='dark'}" @click="appearance='dark'">深色</button><button :class="{active:renderedAppearance==='light'}" @click="appearance='light'">浅色</button></div><div class="segmented" aria-label="材质比较"><button :class="{active:material==='frosted'}" @click="material='frosted'">毛玻璃</button><button :class="{active:material==='liquid'}" @click="material='liquid'">液态玻璃</button></div><select aria-label="桌面背景" x-model="scene"><option value="neutral">中性桌面</option><option value="bright">明亮桌面</option><option value="dark">暗色桌面</option><option value="color">彩色壁纸</option><option value="desk">文字桌面</option></select><label class="solid-check"><input type="checkbox" x-model="solid">减少透明度</label></div></header>"""


def assemble(palette, material, baseline=False):
    document = SOURCE.read_text()
    state_expr = "new URLSearchParams(location.search).get('state')"
    normalized = "(new URLSearchParams(location.search).get('state')||'home').replace(/^(dark|light)-/,'')"
    document = document.replace(state_expr, normalized)
    document = document.replace("appearance:'system'", "appearance:new URLSearchParams(location.search).get('mode')||((new URLSearchParams(location.search).get('state')||'').startsWith('light-')?'light':'dark')")
    data = re.search(r'x-data="([^"]+)"', document).group(1)
    data = data[:-1] + f", material:'{material}', scene:new URLSearchParams(location.search).get('scene')||'neutral', solid:false, get renderedAppearance(){{return this.appearance==='system'?(window.matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light'):this.appearance}}}}"
    document = re.sub(r' x-data="[^"]+"', '', document, count=1)
    body = f'<body data-palette="{palette}" data-material="{material}" data-appearance="dark" data-scene="neutral" data-solid="false" x-data="{data}" :data-appearance="renderedAppearance" :data-material="material" :data-scene="scene" :data-solid="solid">'
    title = "原石墨 · 不透明" if baseline else PALETTES[palette][0] + " · " + ("毛玻璃" if material == 'frosted' else "液态玻璃")
    controls = f'<header class="comparison-controls"><div class="comparison-title">{title}<small>已选定布局的原始配色</small></div></header>' if baseline else CONTROLS.replace('TITLE', '<span x-text="\'' + PALETTES[palette][0] + ' · \' + (material===\'frosted\'?\'毛玻璃\':\'液态玻璃\')">' + title + '</span>')
    document = document.replace('<body>', body + DESKTOP + controls)
    extra_css = COMMON
    if not baseline:
        extra_css += (ROOT / 'materials/frosted.css').read_text()
        extra_css += (ROOT / 'materials/liquid.css').read_text()
    document = document.replace('</style>', extra_css + '</style>')
    document = re.sub(r'<title>.*?</title>', '<title>Flashcast — ' + title + '</title>', document)
    name = 'baseline' if baseline else f'{palette}-{material}'
    (ROOT / (name + '.html')).write_text(document)
    return name, title


if __name__ == '__main__':
    assemble('silver', 'solid', True)
    candidates = [{"id": "baseline", "name": "原石墨 · 不透明", "concept": "已选定的石墨布局与原始银紫配色，放在相同桌面背景上作为材质基线。", "typography": "系统无衬线；输入19px / 结果14px / 设置23px", "palette": ["#1c1d22", "#343b52", "#c5d0ff"], "traits": ["原始不透明表面", "640×420 石墨结构"], "kind": "html", "source": "baseline.html", "baseline": True}]
    for material in ('frosted', 'liquid'):
        for palette, (cn, en, colors, description) in PALETTES.items():
            name, title = assemble(palette, material)
            candidates.append({"id": name, "name": title, "concept": description, "typography": "系统无衬线；输入19px / 结果14px / 设置23px", "palette": colors, "traits": ["深色 / 浅色成对", "同色可直接切换两种材质", "五种桌面背景与减少透明度"], "kind": "html", "source": name + '.html', "interactive": True})
    manifest = {"schemaVersion": 1, "project": "Flashcast · 石墨配色与玻璃", "round": "03", "brief": "三套闪电配色，每套深浅成对；先比较01–03毛玻璃与04–06液态玻璃，再进单张查看切换深浅、材质、桌面背景。液态玻璃为浏览器外观近似，保持石墨640×420布局。", "candidates": candidates}
    (ROOT / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')
