我想做一个类似 raycast 的软件，名为 flashcast，功能需求如下：  
1. 跨平台：支持 linux、mac、windows
2. 类 raycast 的设计风格
3. 快捷键
	1. 启动
		1. linux: alt + space 启动
		2. mac: option + space 启动
		3. windows: alt + space 启动
	2. 移动光标：支持通过类似 vim 风格的方式移动光标去选择搜索结果，不仅仅是 flashcast 自身支持，插件自身页面的光标移动也支持
		1. linux: ctrl + j/k/h/l：下/上/左/右移动光标
		2. mac: control + j/k/h/l：下/上/左/右移动光标
		3. windows: ctrl + j/k/h/l：下/上/左/右移动光标
4. 工作区管理机制：类似 obsidian 的那种关联一个 git 仓库，可以把整个软件使用的各种配置都管理在仓库里
5. 插件机制
	1. 类似 dsh 用的 cordis 插件机制，支持安装各种外部插件，插件自身的配置也是维护在工作区内
	2. 插件本身可配置关键字，用于在 flashcast 输入相关关键字时匹配，匹配中后回车即可进入插件自身的主页面
	3. flashcast 搜索软件、匹配插件关键字时都用模糊匹配，比如：
		1. 软件名为 foo-bar-baz，输入 fbb、ooar、aaz 都能匹配中
		2. 插件关键字是 chrome-bookmarks，输入 book、bookmarks、chbook、chromebook 等都能匹配中
	4. 进入插件内后，flashcast 输入框末尾支持展示当前处在的插件 icon，如果插件没配置 icon 就兜底展示一个 icon，点击 icon 可以进入插件自身的设置页面
	5. 插件自身的设置页面由插件自己维护，不在 flashcast 本身的设置页面里体现，flashcast 的设置页面只有管理插件（安装、更新、卸载等）
	6. 进入插件后：
		1. 默认展示插件的主页面在 flashcast 搜索输入框下方，并且 flashcast 输入框可以继续被插件用来搜索（如果快捷需要用的话）
		2. 按下 esc 可退出插件，回到 flashcast 搜索输入状态
6. 丝滑动画
	1. 打开 flashcast 时要有很丝滑的出现动画，不是简单的淡入淡出，我想要类似 macos 27 自带的 command + space 的那种出现动画，那种很丝滑的流体动画的感觉
	2. esc 关闭 flashcast 时也要有很丝滑流体的退场动画
	3. 进入插件主页面时，也要有很丝滑流体的动画把 flashcast 下方原本的列表收起，再丝滑流体动画去展示插件主页面
	4. 退出插件时也是丝滑流体动画把插件主页面收起，再丝滑流体动画呈现出 flashcast 原本的内容
7. 软件本身内置一些插件，内置的插件如下：
	1. chrome-bookmarks
		1. 匹配关键字：chrome-bookmarks、浏览器书签、
		2. 功能
			1. 设置页面
				1. 支持配置关联哪个 chrome profile，默认用 chrome Default profile
			2. 主页面
				1. 以一个列表展示关联的 chrome profile 下的所有书签，并且同样支持走 flashcast 的搜索输入框继续模糊匹配去搜索相关书签，回车后在 chrome 打开对应书签链接
	2. clipboard
		1. 匹配关键字：剪切板、clipboard、剪贴板
		2. 功能
			1. 支持匹配关键字触发或直接通过快捷键触发
			2. 快捷键
				1. 触发到插件页面
					1. linux: ctrl + alt + c 启动
					2. mac: control + command + c 启动
					3. windows: ctrl + alt + c 启动
				2. 在插件页面内
					1. 清空全部剪切板项
						1. linux: super + alt + backspace
						2. mac: option + command + backspace
						3. windows: win + alt + backspace
					2. 删除光标指向的剪切板项
						1. linux: ctrl + backspace
						2. mac: control + backspace
						3. windows: ctrl + backspace
			3. 设置页面
				1. 支持配置剪切板最大记录容量，默认 10 条，超出的不记录
			4. 主页面
				1. 可以查看最近复制过的内容，包括文本、图片、视频、音乐
				2. 每种类型的剪切板项在列表里有各自的展示规则，光标停留在剪切板项上超过 1.5s 后，有丝滑动画去展开一个右侧面板预览剪切板项的内容：
					1. 文本：
						1. 剪切板列表内展示：正常展示文本，最多展示两行，超过后末尾以 ... 省略
						2. 右侧预览面板：展示完整文本
					2. 图片：
						1. 剪切板列表内展示：图片缩略图
						2. 右侧预览面板：完整图片
					3. 视频：
						1. 剪切板列表内展示：视频封面缩略图
						2. 右侧预览面板：展示视频封面图即可，不需要支持播放视频
					4. 音乐：
						1. 剪切板列表内展示：就展示个『音乐』文案即可
						2. 右侧预览面板：展示个 icon 表明这个剪切板项是一个音乐
				3. 光标在剪切板项上时直接回车：将剪切板项移到最新剪切板项前面，丝滑流体动画关闭 flashcast，这时系统剪切板最新的就是刚刚回车的那个剪切板项，但不需要实际触发粘贴行为
				4. 光标在剪切板项上时按下 option + 回车：同直接回车，但额外会触发一次系统的粘贴行为
	