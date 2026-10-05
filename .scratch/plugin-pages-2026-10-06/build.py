from pathlib import Path
import json, html
root=Path(__file__).parent
directions=[
('01','顺手双栏','列表与正文位置稳定，用最少的切换完成取用。','inline',['列表 42% / 正文 58%','正文区域内联编辑','结果行与原位动作']),
('02','路径导航','把进入与返回做成可读的路径，避免搜索范围混淆。','full',['独立路径栏与搜索栏','全页编辑，保存后返回原选中项','文件夹式阅读顺序']),
('03','动作托盘','搜索与选中保持干净，管理动作集中在键盘面板。','modal',['首页通栏清单','底部动作面板 Ctrl+K','居中编辑对话框']),
('04','内容画廊','以对象形状与缩略图识别入口，鼠标选择更直观。','drawer',['首页双列对象卡片','图片缩略图进入列表','右侧抽屉编辑']),
('05','紧凑清单','一次看到更多结果，按编号快速扫读。','modal',['表格式单列入口','紧凑行高与固定编号','独立编辑对话框']),
('06','焦点工作台','让当前内容占主要空间，确认后再粘贴。','inline',['首页选中对象放大','窄列表与宽正文','阅读区域原位编辑']),
('07','侧边动作','正文保持完整，管理操作沿右边缘展开。','drawer',['专用侧边操作轨道','新建与编辑不占用标题行','编辑抽屉保留左侧上下文']),
('08','分组目录','用类型、文件夹与标签把内容分段，便于浏览。','inline',['入口按软件/插件/标签分组','书签按目录分组','备忘录原位编辑']),
('09','时间线','按最近使用与复制时间扫读，保留内容顺序。','full',['独立时间列与连续轨道','时间标记对齐正文','全页编辑减少空间争抢']),
('10','命令终端','用路径、编号和文字动作维持键盘操作节奏。','modal',['命令提示符与编号','低图标密度、等宽辅助文字','键盘动作面板与编辑对话框'])
]
form='''<form class="editor" @submit.prevent="saveMemo()" data-testid="memo-editor"><h2 x-text="editor?.id?'编辑备忘录':'新建备忘录'"></h2>
<label>标题<input x-ref="title" data-testid="memo-title" :value="editor?.title||''" @input="editor.title=$event.target.value" placeholder="常用回复"></label>
<label>标签<input data-testid="memo-tags" :value="editor?.tags||''" @input="editor.tags=$event.target.value" placeholder="回复、工作"></label>
<label class="body-field">正文<textarea data-testid="memo-body" :value="editor?.body||''" @input="editor.body=$event.target.value" placeholder="输入要粘贴的内容…"></textarea></label>
<p class="editor-error" x-show="editor?.error" x-text="editor?.error" role="alert"></p>
<div class="form-buttons"><button type="button" class="ghost" @click="editor=null">取消</button><button type="submit" class="primary" data-testid="memo-save">保存</button></div></form>'''
template=(root/'template.html').read_text()
css=(root/'prototype.css').read_text()
script=(root/'prototype.js').read_text()
alpine=(root/'assets/alpine.min.js').read_text()
manifest={'schemaVersion':1,'project':'Flashcast · 插件页面十版交互','brief':'搜索后回车进入插件；剪切板左历史右预览；备忘录管理与平台快捷键配置。点击单张即可操作。','round':'交互 01','candidates':[]}
# 当前截图仅作为基线，不计入十个候选。
manifest['candidates'].append({'id':'current','name':'当前主页面','concept':'对照移除顶部插件 Tab 前的当前主页面。','typography':'系统无衬线','palette':['#f3f8fd','#2c6095'],'traits':['顶部插件 Tab','直接展示备忘录内容'],'kind':'image','source':'assets/baseline.png','baseline':True})
for ident,name,concept,editor,traits in directions:
 config={'id':ident,'name':name,'editor':editor}
 fallback=''.join(f'<div style="padding:12px 10px;border-bottom:1px solid #dbe4eb;font-size:14px">{title}<small style="display:block;color:#657b8e;margin-top:4px;font-size:12px">{subtitle}</small></div>' for title,subtitle in [('Visual Studio Code','代码编辑器'),('文件','浏览文件与目录'),('剪切板','查看复制过的文字与图片'),('备忘录','常用文字与标签'),('Chrome 书签','打开收藏的链接')])
 values={'TITLE':name,'ID':ident,'CONFIG':html.escape(json.dumps(config,ensure_ascii=False),quote=True),'CSS':css,'FALLBACK':fallback,'APP_SCRIPT':script,'ALPINE':alpine,'INLINE_EDITOR':form if editor=='inline' else '', 'OVERLAY_EDITOR':form if editor=='modal' else '', 'DRAWER_EDITOR':form if editor=='drawer' else '', 'FULL_EDITOR':form if editor=='full' else ''}
 out=template
 for key,value in values.items():out=out.replace('__'+key+'__',value)
 (root/(ident+'.html')).write_text(out)
 manifest['candidates'].append({'id':'v'+ident,'name':name,'concept':concept,'typography':('等宽辅助文字 / 系统中文；正文14px、标题17px' if ident=='10' else '系统中文无衬线；正文14px、标题15–20px'),'palette':['#f8fbfd','#263748','#66dcf1'],'traits':traits,'kind':'html','source':ident+'.html','interactive':True})
(root/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n')
