/**
 * macOS 应用的展示名与搜索别名。
 *
 * 应用目录里的 bundle 用的是英文文件名（`Finder.app`、`Maps.app`、`System Settings.app`），
 * 中文名（访达、地图、系统设置）只写在 bundle 的本地化资源里。启动器界面只有中文，
 * 直接拿文件名当应用名既不像 macOS，也会让「访达」这类搜索落空。
 *
 * 本地化名有两种存法，都是 plist：
 * - 第三方应用：`Contents/Resources/<lang>.lproj/InfoPlist.strings`
 * - 系统应用：`Contents/Resources/InfoPlist.loctable`，顶层按语言分字典
 * 系统自带的 `plutil` 能把 binary plist 转成 JSON，不必自己解析二进制格式。
 */

import { execFile } from 'node:child_process'
import { existsSync } from 'node:fs'
import { basename, join } from 'node:path'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

/** 界面只做中文，按这个顺序找中文本地化资源。 */
const NAME_LOCALES = ['zh_CN', 'zh-Hans', 'zh'] as const
/** 按优先级排列：应用在 Finder / Dock 里显示的名字优先。 */
const NAME_KEYS = ['CFBundleDisplayName', 'CFBundleName'] as const

export interface MacAppName {
  /** 展示名，有中文本地化名时用它。 */
  name: string
  /** 搜索别名：展示名换成中文后，英文 bundle 名仍然要能搜到。 */
  keywords: string[]
}

/** 从 plist 字典里取 bundle 名，取不到时返回空串。 */
export function pickBundleName(values: unknown): string {
  if (!values || typeof values !== 'object') return ''
  for (const key of NAME_KEYS) {
    const value = (values as Record<string, unknown>)[key]
    if (typeof value === 'string' && value.trim()) return value.trim()
  }
  return ''
}

/**
 * 把 plist 转成 JSON。`language` 用于多语言字典（loctable）：只抽出对应语言那一层，
 * 避免把整份资源都读进来。文件不是 plist、缺少该语言或内容转不成 JSON 都返回 null。
 */
async function readPlistJson(file: string, language?: string): Promise<unknown> {
  const args = language
    ? ['-extract', language, 'json', '-o', '-', file]
    : ['-convert', 'json', '-o', '-', file]
  try {
    const { stdout } = await execFileAsync('/usr/bin/plutil', args, {
      timeout: 3000,
      maxBuffer: 4 * 1024 * 1024
    })
    return JSON.parse(stdout) as unknown
  } catch {
    return null
  }
}

async function localizedAppName(appPath: string): Promise<string> {
  const resources = join(appPath, 'Contents/Resources')
  for (const locale of NAME_LOCALES) {
    const strings = join(resources, `${locale}.lproj/InfoPlist.strings`)
    if (!existsSync(strings)) continue
    const name = pickBundleName(await readPlistJson(strings))
    if (name) return name
  }
  const loctable = join(resources, 'InfoPlist.loctable')
  if (existsSync(loctable)) {
    for (const locale of NAME_LOCALES) {
      const name = pickBundleName(await readPlistJson(loctable, locale))
      if (name) return name
    }
  }
  return ''
}

/**
 * 应用的展示名和搜索别名。中文名展示时保留英文 bundle 名做关键字，
 * 这样「Finder」「Lark」「WeChat」这类英文输入仍然能命中；没有中文资源时行为不变。
 */
export async function resolveMacAppName(appPath: string): Promise<MacAppName> {
  const bundleName = basename(appPath, '.app')
  const localized = await localizedAppName(appPath)
  if (!localized || localized === bundleName) return { name: bundleName, keywords: [] }
  return { name: localized, keywords: [bundleName] }
}
