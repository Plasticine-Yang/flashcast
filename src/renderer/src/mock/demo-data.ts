import type {
  AppEntry,
  BootstrapData,
  ChromeBookmark,
  ChromePluginConfig,
  ChromeProfile,
  ClipboardItem,
  ClipboardPluginConfig,
  CommandEntry,
  EditorVariant,
  EditorWorkspace,
  GenericPluginSettingsPage,
  PluginSettingsPage,
  VscodeLikeWorkspaceSettingsPage,
  PluginSummary,
  ShortcutInfo,
  WorkspaceSummary
} from '@shared/types'

const now = Date.now()

export const mockSvg = (markup: string, width = 160, height = 120): string =>
  `data:image/svg+xml;charset=utf-8,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">${markup}</svg>`
  )}`

const appIcon = (from: string, to: string, glyph: string): string =>
  mockSvg(
    `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="${from}"/><stop offset="1" stop-color="${to}"/></linearGradient></defs>
     <rect width="64" height="64" rx="15" fill="url(#g)"/>
     <text x="32" y="42" text-anchor="middle" font-family="Inter, sans-serif" font-size="28" font-weight="700" fill="#fff">${glyph}</text>`,
    64,
    64
  )

export const mockApps: AppEntry[] = [
  {
    id: 'app-chrome',
    name: 'Google Chrome',
    subtitle: '浏览器',
    keywords: ['chrome', '浏览器', 'browser'],
    icon: appIcon('#3b82f6', '#22c55e', 'C'),
    iconKey: 'app'
  },
  {
    id: 'app-code',
    name: 'Visual Studio Code',
    subtitle: '开发',
    keywords: ['vscode', 'code', 'editor', '编辑器'],
    icon: appIcon('#2563eb', '#0ea5e9', 'V'),
    iconKey: 'app'
  },
  {
    id: 'app-figma',
    name: 'Figma',
    subtitle: '设计',
    keywords: ['design', 'ui', '设计'],
    icon: appIcon('#f97316', '#ef4444', 'F'),
    iconKey: 'app'
  },
  {
    id: 'app-terminal',
    name: 'Terminal',
    subtitle: '终端',
    keywords: ['shell', 'bash', '终端', '命令行'],
    icon: appIcon('#3f3f46', '#18181b', '>'),
    iconKey: 'app'
  },
  {
    id: 'app-files',
    name: 'Files',
    subtitle: '文件',
    keywords: ['nautilus', 'finder', '文件', 'folder'],
    icon: appIcon('#0ea5e9', '#0284c7', 'F'),
    iconKey: 'app'
  },
  {
    id: 'app-spotify',
    name: 'Spotify',
    subtitle: '影音',
    keywords: ['music', '音乐', 'player'],
    icon: appIcon('#22c55e', '#15803d', 'S'),
    iconKey: 'app'
  },
  {
    id: 'app-slack',
    name: 'Slack',
    subtitle: '通讯',
    keywords: ['chat', 'team', '消息'],
    icon: appIcon('#7c3aed', '#db2777', 'S'),
    iconKey: 'app'
  },
  {
    id: 'app-notion',
    name: 'Notion',
    subtitle: '办公',
    keywords: ['notes', 'wiki', '笔记'],
    icon: appIcon('#f5f5f5', '#d4d4d8', 'N'),
    iconKey: 'app'
  },
  {
    id: 'app-obsidian',
    name: 'Obsidian',
    subtitle: '办公',
    keywords: ['notes', 'markdown', '笔记'],
    icon: appIcon('#6d28d9', '#a78bfa', 'O'),
    iconKey: 'app'
  },
  {
    id: 'app-settings',
    name: 'Settings',
    subtitle: '系统',
    keywords: ['preferences', '设置', '系统'],
    icon: appIcon('#52525b', '#3f3f46', 'S'),
    iconKey: 'app'
  }
]

export const mockCommands: CommandEntry[] = [
  {
    id: 'settings',
    name: '打开设置',
    subtitle: '主题、插件、工作区与快捷键',
    keywords: ['设置', 'settings', 'preferences', '配置'],
    iconKey: 'settings'
  },
  {
    id: 'check-updates',
    name: '检查更新',
    subtitle: '获取新版本，并在有更新时一键安装',
    keywords: ['更新', '升级', '新版本', 'update', 'upgrade', 'check'],
    iconKey: 'update'
  },
  {
    id: 'refresh-apps',
    name: '重新加载应用',
    subtitle: '刷新可搜索的已安装应用',
    keywords: ['刷新', 'reload', 'refresh', '应用'],
    iconKey: 'refresh'
  },
  {
    id: 'quit',
    name: '退出 Flashcast',
    subtitle: '完全退出应用',
    keywords: ['退出', 'quit', 'exit'],
    iconKey: 'power'
  }
]

export const mockPlugins: PluginSummary[] = [
  {
    id: 'clipboard',
    name: '剪切板',
    description: '最近复制的内容，随时取回',
    version: '1.0.0',
    keywords: ['剪切板', '剪贴板', 'clipboard', '粘贴板', '复制历史'],
    iconKey: 'clipboard',
    searchable: true,
    primaryAction: '置顶并关闭',
    builtin: true,
    hasSettings: true,
    permissions: [],
    layout: 'compact',
    enabled: true,
    status: 'ok',
    source: 'builtin'
  },
  {
    id: 'vscode-like-workspace',
    name: 'VSCode-like Workspace',
    description: '搜索 VS Code、Trae、Cursor 等打开过的工作区',
    version: '1.0.0',
    keywords: ['编辑器', '工作区', 'workspace', '项目', 'editor', 'vscode-like'],
    iconKey: 'code',
    searchable: true,
    primaryAction: '打开工作区',
    builtin: true,
    hasSettings: true,
    permissions: [],
    layout: 'compact',
    enabled: true,
    status: 'ok',
    source: 'builtin',
    triggers: [
      { keyword: 'vsc', label: 'Visual Studio Code 工作区', args: { editor: 'code' } },
      { keyword: 'trae', label: 'Trae CN 工作区', args: { editor: 'trae-cn' } }
    ]
  },
  {
    id: 'chrome-bookmarks',
    name: 'Chrome 书签',
    description: '搜索并打开 Chrome 书签',
    version: '1.0.0',
    keywords: ['chrome-bookmarks', '浏览器书签', '书签', 'bookmarks', 'chrome', '浏览器'],
    iconKey: 'bookmark',
    searchable: true,
    primaryAction: '在 Chrome 打开',
    builtin: true,
    hasSettings: true,
    permissions: [],
    layout: 'compact',
    enabled: true,
    status: 'ok',
    source: 'builtin'
  }
]

export const mockChromeProfiles: ChromeProfile[] = [
  {
    dir: 'Default',
    name: 'Default',
    bookmarksPath: '/home/you/.config/google-chrome/Default/Bookmarks'
  },
  {
    dir: 'Profile 1',
    name: 'Work',
    bookmarksPath: '/home/you/.config/google-chrome/Profile 1/Bookmarks'
  }
]

export const mockClipboardItems: ClipboardItem[] = [
  {
    id: 'clip-text-1',
    kind: 'text',
    createdAt: now - 42 * 1000,
    text: 'npm run dev — 同时在 out/main 与 out/renderer 构建，热更新会保留工作区里的插件配置。',
    title: 'npm run dev — 同时在 out/main 与 out/renderer 构建，热更新会保留工作区里的插件配置。',
    byteSize: 104
  },
  {
    id: 'clip-image-1',
    kind: 'image',
    createdAt: now - 3 * 60 * 1000,
    title: '图片',
    mediaPath: mockSvg(
      `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#0f172a"/><stop offset=".55" stop-color="#334155"/><stop offset="1" stop-color="#f59e0b"/></linearGradient></defs>
       <rect width="900" height="600" fill="url(#g)"/>
       <circle cx="680" cy="170" r="86" fill="#fbbf24" opacity=".9"/>
       <path d="M0 470 L210 310 L410 420 L640 220 L900 390 V600 H0 Z" fill="#0b1120" opacity=".78"/>
       <path d="M0 520 L260 400 L520 500 L760 360 L900 430 V600 H0 Z" fill="#020617" opacity=".6"/>`,
      900,
      600
    ),
    thumbPath: mockSvg(
      `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#0f172a"/><stop offset=".55" stop-color="#334155"/><stop offset="1" stop-color="#f59e0b"/></linearGradient></defs>
       <rect width="160" height="160" fill="url(#g)"/><circle cx="118" cy="42" r="22" fill="#fbbf24"/>`,
      160,
      160
    ),
    width: 900,
    height: 600,
    byteSize: 1_842_400
  },
  {
    id: 'clip-video-1',
    kind: 'video',
    createdAt: now - 8 * 60 * 1000,
    title: '视频',
    fileName: 'flashcast-launch-teaser.mp4',
    mediaPath: mockSvg(
      `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#0d1828"/><stop offset=".5" stop-color="#13233c"/><stop offset="1" stop-color="#1f3f78"/></linearGradient></defs>
       <rect width="640" height="360" fill="url(#g)"/>
       <circle cx="320" cy="165" r="38" fill="#6fa8ff" opacity=".16" stroke="#6fa8ff" stroke-opacity=".5"/>
       <path d="M310 146 L342 165 L310 184 Z" fill="#9ac4ff"/>
       <text x="320" y="240" text-anchor="middle" fill="#eef4ff" font-family="Inter, sans-serif" font-size="17" font-weight="500">flashcast-launch-teaser.mp4</text>`,
      640,
      360
    ),
    thumbPath: mockSvg(
      `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#0d1828"/><stop offset="1" stop-color="#1f3f78"/></linearGradient></defs>
       <rect width="160" height="160" fill="url(#g)"/><circle cx="80" cy="80" r="24" fill="#6fa8ff" opacity=".2"/><path d="M72 66 L94 80 L72 94 Z" fill="#9ac4ff"/>`,
      160,
      160
    ),
    byteSize: 48_211_000
  },
  {
    id: 'clip-text-2',
    kind: 'text',
    createdAt: now - 17 * 60 * 1000,
    text: 'https://raycast.com/blog/spring-animations',
    title: 'https://raycast.com/blog/spring-animations',
    byteSize: 48
  },
  {
    id: 'clip-music-1',
    kind: 'music',
    createdAt: now - 26 * 60 * 1000,
    title: '音乐',
    fileName: 'Nightdrive — Aurora.flac',
    byteSize: 32_401_280
  },
  {
    id: 'clip-text-3',
    kind: 'text',
    createdAt: now - 51 * 60 * 1000,
    text: 'feat(launcher): add spring-based plugin transition\n\n- stack plugin view above results\n- animate blur + translate only\n- respect reduced motion',
    title: 'feat(launcher): add spring-based plugin transition',
    byteSize: 162
  }
]

export const mockBookmarks: ChromeBookmark[] = [
  {
    id: 'bm-github',
    title: 'GitHub — Personal Access Tokens',
    url: 'https://github.com/settings/tokens',
    folder: ['工作', '开发'],
    domain: 'github.com'
  },
  {
    id: 'bm-mdn',
    title: 'MDN Web Docs',
    url: 'https://developer.mozilla.org/en-US/',
    folder: ['学习', '前端'],
    domain: 'developer.mozilla.org'
  },
  {
    id: 'bm-figma',
    title: 'Figma — Flashcast',
    url: 'https://figma.com/file/flashcast',
    folder: ['设计'],
    domain: 'figma.com'
  },
  {
    id: 'bm-linear',
    title: 'Linear — Flashcast roadmap',
    url: 'https://linear.app/flashcast/roadmap',
    folder: ['工作'],
    domain: 'linear.app'
  },
  {
    id: 'bm-notion',
    title: 'Notion — Product spec',
    url: 'https://notion.so/flashcast/product-spec',
    folder: ['工作', '文档'],
    domain: 'notion.so'
  },
  {
    id: 'bm-raycast',
    title: 'Raycast — Keyboard-first design',
    url: 'https://raycast.com/blog',
    folder: ['灵感'],
    domain: 'raycast.com'
  },
  {
    id: 'bm-stripe',
    title: 'Stripe API Reference',
    url: 'https://stripe.com/docs/api',
    folder: ['工作', '开发'],
    domain: 'stripe.com'
  },
  {
    id: 'bm-youtube',
    title: 'Fluid interfaces — talk',
    url: 'https://youtube.com/watch?v=fluid-interfaces',
    folder: ['学习'],
    domain: 'youtube.com'
  }
]

const FAVICON_COLORS: Record<string, string> = {
  'github.com': '#e5e7eb',
  'developer.mozilla.org': '#fbbf24',
  'figma.com': '#f97316',
  'linear.app': '#a78bfa',
  'notion.so': '#f4f4f5',
  'raycast.com': '#ff6363',
  'stripe.com': '#818cf8',
  'youtube.com': '#ef4444'
}

export function mockFavicons(domains: string[]): Record<string, string> {
  const result: Record<string, string> = {}
  for (const domain of domains) {
    const color = FAVICON_COLORS[domain]
    if (!color) continue
    result[domain] = mockSvg(
      `<circle cx="32" cy="32" r="28" fill="${color}" opacity=".16"/>
       <text x="32" y="43" text-anchor="middle" font-family="Inter, sans-serif" font-size="30" font-weight="700" fill="${color}">${domain[0].toUpperCase()}</text>`,
      64,
      64
    )
  }
  return result
}

export const mockShortcuts: ShortcutInfo[] = [
  {
    id: 'launcher',
    label: '打开 / 关闭 Flashcast',
    description: '在任意应用中唤起启动器',
    accelerator: 'Alt+Space',
    defaultAccelerator: 'Alt+Space',
    editable: true
  },
  {
    id: 'openClipboard',
    label: '打开剪切板',
    description: '直接进入剪切板插件',
    accelerator: 'Control+Alt+C',
    defaultAccelerator: 'Control+Alt+C',
    editable: true
  }
]

export const mockWorkspace: WorkspaceSummary = {
  path: '/home/you/.flashcast',
  exists: true,
  isRepo: true,
  branch: 'main',
  lastCommit: {
    hash: 'e16a9a5',
    message: 'config: update Flashcast workspace',
    date: now - 2 * 60 * 60 * 1000
  },
  dirtyCount: 2,
  conflict: false,
  ahead: 1,
  behind: 0,
  hasRemote: true,
  autoCommit: true,
  syncState: 'dirty',
  lastError: null
}

export const mockClipboardConfig: ClipboardPluginConfig = { maxItems: 10 }
export const mockChromeConfig: ChromePluginConfig = { profile: 'Default' }

export const mockClipboardSettings: GenericPluginSettingsPage = {
  kind: 'settings',
  pluginId: 'clipboard',
  title: '剪切板设置',
  placeholder: '剪切板设置…',
  searchable: false,
  fields: [
    {
      key: 'maxItems',
      type: 'number',
      label: '最大记录容量',
      value: 10,
      min: 1,
      max: 50,
      step: 1,
      unit: '条',
      description: '默认 10 条，超出的最早记录会被自动丢弃'
    }
  ],
  error: null,
  notice: null,
  savedAt: 0
}

export const mockChromeSettings: GenericPluginSettingsPage = {
  kind: 'settings',
  pluginId: 'chrome-bookmarks',
  title: 'Chrome 书签设置',
  placeholder: 'Chrome 书签设置…',
  searchable: false,
  fields: [
    {
      key: 'profile',
      type: 'select',
      label: 'Chrome Profile',
      value: 'Default',
      options: mockChromeProfiles.map((profile) => ({
        value: profile.dir,
        label: profile.dir === 'Default' ? 'Default' : `${profile.name} (${profile.dir})`
      })),
      description: '默认使用 Default Profile；切换后立即生效'
    },
    {
      key: 'profilePath',
      type: 'text',
      label: 'Profile 路径',
      value: '/home/you/.config/google-chrome/Default',
      mono: true,
      readonly: true,
      description: '该 Profile 的本地数据目录'
    },
    {
      key: 'bookmarkFile',
      type: 'text',
      label: '书签文件',
      value: '/home/you/.config/google-chrome/Default/AccountBookmarks',
      mono: true,
      readonly: true,
      description: 'Chrome 把书签保存在登录的 Google 账户里（AccountBookmarks）'
    }
  ],
  error: null,
  notice: null,
  savedAt: 0
}

export const mockBootstrap: BootstrapData = {
  role: 'launcher',
  platform: 'linux',
  version: '0.1.0',
  prefs: {
    theme: 'dark',
    launchAtLogin: false,
    language: 'zh-CN',
    autoCommit: true,
    hotkeys: { launcher: 'Alt+Space', openClipboard: 'Control+Alt+C' },
    enabledPlugins: { clipboard: true, 'chrome-bookmarks': true }
  },
  theme: 'dark',
  shortcuts: mockShortcuts,
  shortcutErrors: [],
  plugins: mockPlugins,
  commands: mockCommands,
  workspace: mockWorkspace,
  chromeProfiles: mockChromeProfiles,
  launchAtLoginSupported: true,
  dataDir: '/home/you/.flashcast',
  isDev: true
}

/* ------------------------------------------------------------------------- */
/* Preview desktop wallpaper                                                   */
/* ------------------------------------------------------------------------- */

function wallpaperDataUrl(svg: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`
}

/**
 * The preview harness has no desktop to sample, so it uses a generated
 * wallpaper that matches the preview background. The launcher blurs and
 * color-blends it exactly like the real GNOME/Windows wallpaper, which keeps
 * the browser preview faithful to the Electron window.
 */
export const mockWallpaperDark = wallpaperDataUrl(`
<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="872" viewBox="0 0 1200 872">
  <defs>
    <linearGradient id="base" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0a1428"/>
      <stop offset="0.44" stop-color="#0d1c38"/>
      <stop offset="1" stop-color="#071019"/>
    </linearGradient>
    <radialGradient id="blue">
      <stop offset="0" stop-color="#4080ff" stop-opacity="0.62"/>
      <stop offset="1" stop-color="#4080ff" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="cyan">
      <stop offset="0" stop-color="#38c8eb" stop-opacity="0.4"/>
      <stop offset="1" stop-color="#38c8eb" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="warm">
      <stop offset="0" stop-color="#ffac5c" stop-opacity="0.3"/>
      <stop offset="1" stop-color="#ffac5c" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="rose">
      <stop offset="0" stop-color="#ff8a8a" stop-opacity="0.55"/>
      <stop offset="1" stop-color="#ff8a8a" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="1200" height="872" fill="url(#base)"/>
  <ellipse cx="170" cy="35" rx="470" ry="360" fill="url(#blue)"/>
  <ellipse cx="1030" cy="105" rx="380" ry="300" fill="url(#cyan)"/>
  <ellipse cx="620" cy="400" rx="300" ry="240" fill="url(#rose)"/>
  <ellipse cx="700" cy="925" rx="420" ry="360" fill="url(#warm)"/>
</svg>`)

export const mockWallpaperLight = wallpaperDataUrl(`
<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="872" viewBox="0 0 1200 872">
  <defs>
    <linearGradient id="base" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#eaf3ff"/>
      <stop offset="0.46" stop-color="#f4f8ff"/>
      <stop offset="1" stop-color="#e6f1fb"/>
    </linearGradient>
    <radialGradient id="blue">
      <stop offset="0" stop-color="#609cff" stop-opacity="0.5"/>
      <stop offset="1" stop-color="#609cff" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="cyan">
      <stop offset="0" stop-color="#7dd3fc" stop-opacity="0.5"/>
      <stop offset="1" stop-color="#7dd3fc" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="warm">
      <stop offset="0" stop-color="#ffc480" stop-opacity="0.38"/>
      <stop offset="1" stop-color="#ffc480" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="rose">
      <stop offset="0" stop-color="#ff9d9d" stop-opacity="0.45"/>
      <stop offset="1" stop-color="#ff9d9d" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="1200" height="872" fill="url(#base)"/>
  <ellipse cx="170" cy="35" rx="470" ry="360" fill="url(#blue)"/>
  <ellipse cx="1030" cy="105" rx="380" ry="300" fill="url(#cyan)"/>
  <ellipse cx="620" cy="400" rx="300" ry="240" fill="url(#rose)"/>
  <ellipse cx="700" cy="925" rx="420" ry="360" fill="url(#warm)"/>
</svg>`)

/* --------------------------- VS Code-like editors -------------------------- */

export const mockEditorVariants: EditorVariant[] = [
  {
    id: 'code',
    name: 'Visual Studio Code',
    version: '1.105.0',
    dataDir: '/Users/you/Library/Application Support/Code',
    installed: true,
    error: null
  },
  {
    id: 'trae-cn',
    name: 'Trae CN',
    version: '1.107.1',
    dataDir: '/Users/you/Library/Application Support/Trae CN',
    installed: true,
    error: null
  },
  {
    id: 'cursor',
    name: 'Cursor',
    version: '0.49.6',
    dataDir: '/Users/you/Library/Application Support/Cursor',
    installed: true,
    error: null
  }
]

export const mockEditorWorkspaces: EditorWorkspace[] = [
  {
    id: 'ws-trae-corehr',
    variantId: 'trae-cn',
    name: 'corehr-fe',
    path: '/Users/you/code/bytedance-projects/corehr-fe',
    displayPath: '~/code/bytedance-projects/corehr-fe',
    uri: 'file:///Users/you/code/bytedance-projects/corehr-fe',
    kind: 'folder',
    remote: null,
    isOpen: true,
    lastActiveAt: now - 2 * 60 * 1000,
    exists: true
  },
  {
    id: 'ws-trae-bpm',
    variantId: 'trae-cn',
    name: 'bpm-fe',
    path: '/Users/you/code/bytedance-projects/bpm-fe',
    displayPath: '~/code/bytedance-projects/bpm-fe',
    uri: 'file:///Users/you/code/bytedance-projects/bpm-fe',
    kind: 'folder',
    remote: null,
    isOpen: true,
    lastActiveAt: now - 26 * 60 * 1000,
    exists: true
  },
  {
    id: 'ws-trae-remote',
    variantId: 'trae-cn',
    name: 'cloudide-workspace',
    path: '/cloudide/workspace',
    displayPath: 'devbox:/cloudide/workspace',
    uri: 'vscode-remote://ssh-remote%2Bdevbox/cloudide/workspace',
    kind: 'folder',
    remote: { authority: 'ssh-remote+devbox', host: 'devbox', label: 'ssh-remote · devbox' },
    isOpen: false,
    lastActiveAt: now - 2 * 24 * 60 * 60 * 1000,
    exists: true
  },
  {
    id: 'ws-code-kuake',
    variantId: 'code',
    name: '夸克浏览器',
    path: '/Users/you/code/side-projects/kuake-browser',
    displayPath: '~/code/side-projects/kuake-browser',
    uri: 'file:///Users/you/code/side-projects/kuake-browser',
    kind: 'folder',
    remote: null,
    isOpen: false,
    lastActiveAt: now - 4 * 60 * 60 * 1000,
    exists: true
  },
  {
    id: 'ws-code-plasticine',
    variantId: 'code',
    name: 'plasticine',
    path: '/Users/you/code/plasticine/plasticine.code-workspace',
    displayPath: '~/code/plasticine/plasticine.code-workspace',
    uri: 'file:///Users/you/code/plasticine/plasticine.code-workspace',
    kind: 'workspace',
    remote: null,
    isOpen: false,
    lastActiveAt: now - 3 * 24 * 60 * 60 * 1000,
    exists: true
  },
  {
    id: 'ws-code-missing',
    variantId: 'code',
    name: 'legacy-admin',
    path: '/Users/you/code/archive/legacy-admin',
    displayPath: '~/code/archive/legacy-admin',
    uri: 'file:///Users/you/code/archive/legacy-admin',
    kind: 'folder',
    remote: null,
    isOpen: false,
    lastActiveAt: now - 40 * 24 * 60 * 60 * 1000,
    exists: false
  },
  {
    id: 'ws-cursor-design',
    variantId: 'cursor',
    name: 'design-system',
    path: '/Users/you/code/design-system',
    displayPath: '~/code/design-system',
    uri: 'file:///Users/you/code/design-system',
    kind: 'folder',
    remote: null,
    isOpen: false,
    lastActiveAt: now - 6 * 24 * 60 * 60 * 1000,
    exists: true
  }
]

export const mockEditorSettings: VscodeLikeWorkspaceSettingsPage = {
  kind: 'vscode-like-workspace-settings',
  pluginId: 'vscode-like-workspace',
  title: 'VSCode-like Workspace 设置',
  placeholder: 'VSCode-like Workspace 设置…',
  searchable: false,
  variants: [
    {
      id: 'code',
      name: 'Visual Studio Code',
      version: '1.105.0',
      dataDir: '/Users/you/Library/Application Support/Code',
      trigger: 'vsc',
      enabled: true
    },
    {
      id: 'trae-cn',
      name: 'Trae CN',
      version: '1.107.1',
      dataDir: '/Users/you/Library/Application Support/Trae CN',
      trigger: 'trae',
      enabled: true
    },
    {
      id: 'cursor',
      name: 'Cursor',
      version: '0.49.6',
      dataDir: '/Users/you/Library/Application Support/Cursor',
      trigger: '',
      enabled: true
    }
  ],
  options: { includeRemote: true, hideMissing: false, maxItems: 1000 },
  triggerErrors: {},
  workspaceStats: {
    code: { total: 3, missing: 1, remote: 0, running: false },
    'trae-cn': { total: 3, missing: 0, remote: 1, running: false },
    cursor: { total: 1, missing: 0, remote: 0, running: false }
  },
  error: null,
  notice: null,
  savedAt: now
}
