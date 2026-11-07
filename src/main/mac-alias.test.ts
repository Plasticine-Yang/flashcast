import { describe, expect, it } from 'vitest'
import { aliasTargetsFromBytes } from './mac-alias'

/** 拼一个结构接近真实替身文件的字节串：`book...mark` 头 + NUL 分隔的路径记录。 */
function aliasFile(urls: string[]): Buffer {
  const header = Buffer.from('book\0\0\0\0mark\0\0\0\0', 'latin1')
  const body = Buffer.concat([
    Buffer.from('\x01\x01\0\0Applications\0\0', 'latin1'),
    ...urls.flatMap((url) => [Buffer.from(url, 'latin1'), Buffer.alloc(4)])
  ])
  return Buffer.concat([header, body])
}

describe('aliasTargetsFromBytes', () => {
  it('解出替身文件记录的 bundle 路径', () => {
    const raw = aliasFile([
      'file:///Applications/Doubao.app/Contents/Helpers/Doubao%20Browser.app'
    ])
    expect(aliasTargetsFromBytes(raw)).toEqual([
      '/Applications/Doubao.app/Contents/Helpers/Doubao Browser.app'
    ])
  })

  it('嵌套 .app 路径取完整路径，而不是停在父 bundle', () => {
    const raw = aliasFile(['file:///Applications/DoubaoWork.app/Contents/Helpers/DoubaoWork%20Browser.app'])
    expect(aliasTargetsFromBytes(raw)).toEqual([
      '/Applications/DoubaoWork.app/Contents/Helpers/DoubaoWork Browser.app'
    ])
  })

  it('同一目标只返回一次', () => {
    const url = 'file:///Applications/Safari.app'
    expect(aliasTargetsFromBytes(aliasFile([url, url]))).toEqual(['/Applications/Safari.app'])
  })

  it('不是替身文件（bundle 目录读出的内容、空字节）时不返回候选', () => {
    expect(aliasTargetsFromBytes(Buffer.alloc(0))).toEqual([])
    expect(aliasTargetsFromBytes(Buffer.from('plist\0file:///Applications/Safari.app'))).toEqual([])
  })

  it('跳过非法 URL 编码', () => {
    expect(aliasTargetsFromBytes(aliasFile(['file:///Applications/%E0%A4%A.app']))).toEqual([])
  })
})
