import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { pickBundleName, resolveMacAppName } from './mac-app-name'

const plist = (body: string): string =>
  `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">${body}</plist>`

const stringDict = (entries: Record<string, string>): string =>
  `<dict>${Object.entries(entries)
    .map(([key, value]) => `<key>${key}</key><string>${value}</string>`)
    .join('')}</dict>`

/** loctable 顶层是「语言 → 字符串字典」，这里只铺一层嵌套。 */
const loctable = (languages: Record<string, Record<string, string>>): string =>
  `<dict>${Object.entries(languages)
    .map(([language, entries]) => `<key>${language}</key>${stringDict(entries)}`)
    .join('')}</dict>`

describe('pickBundleName', () => {
  it('优先 CFBundleDisplayName，并去掉首尾空白', () => {
    expect(
      pickBundleName({ CFBundleDisplayName: ' 微信 ', CFBundleName: 'WeChat' })
    ).toBe('微信')
  })

  it('DisplayName 为空时回退 CFBundleName', () => {
    expect(pickBundleName({ CFBundleDisplayName: '  ', CFBundleName: '访达' })).toBe('访达')
  })

  it('忽略非字符串、缺失字段与空值', () => {
    expect(pickBundleName({ CFBundleDisplayName: 42, CFBundleName: '' })).toBe('')
    expect(pickBundleName({})).toBe('')
    expect(pickBundleName(null)).toBe('')
    expect(pickBundleName('Finder')).toBe('')
  })
})

const darwin = process.platform === 'darwin'

/** 在临时目录造一个只有 Info 资源的 .app，交给真正的 plutil 解析。 */
async function fakeApp(root: string, resources: Record<string, string>): Promise<string> {
  const appPath = join(root, 'Sample.app')
  const base = join(appPath, 'Contents/Resources')
  for (const [relative, content] of Object.entries(resources)) {
    const file = join(base, relative)
    await mkdir(dirname(file), { recursive: true })
    await writeFile(file, content)
  }
  return appPath
}

describe.skipIf(!darwin)('resolveMacAppName', () => {
  let root: string | null = null

  afterEach(async () => {
    if (root) await rm(root, { recursive: true, force: true })
    root = null
  })

  async function workspace(): Promise<string> {
    root = await mkdtemp(join(tmpdir(), 'flashcast-app-name-'))
    return root
  }

  it('读取 zh-Hans.lproj/InfoPlist.strings 的显示名，并保留英文名别名', async () => {
    const dir = await workspace()
    const appPath = await fakeApp(dir, {
      'zh-Hans.lproj/InfoPlist.strings': plist(
        stringDict({ CFBundleDisplayName: '示例', CFBundleName: '示例' })
      )
    })
    await expect(resolveMacAppName(appPath)).resolves.toEqual({
      name: '示例',
      keywords: ['Sample']
    })
  })

  it('系统应用从 InfoPlist.loctable 取对应语言的名字', async () => {
    const dir = await workspace()
    const appPath = await fakeApp(dir, {
      'InfoPlist.loctable': plist(
        loctable({
          zh_CN: { CFBundleName: '访达' },
          en: { CFBundleName: 'Finder' }
        })
      )
    })
    await expect(resolveMacAppName(appPath)).resolves.toEqual({
      name: '访达',
      keywords: ['Sample']
    })
  })

  it('没有中文资源时退回 bundle 文件名，不产生多余关键字', async () => {
    const dir = await workspace()
    const appPath = await fakeApp(dir, {})
    await expect(resolveMacAppName(appPath)).resolves.toEqual({
      name: 'Sample',
      keywords: []
    })
  })

  it('Finder 展示为「访达」，英文 Finder 仍可作为关键字命中', async () => {
    await expect(resolveMacAppName('/System/Library/CoreServices/Finder.app')).resolves.toEqual({
      name: '访达',
      keywords: ['Finder']
    })
  })
})
