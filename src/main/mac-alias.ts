/**
 * macOS Finder 的“制作替身”得到的 `.app` 只是几十字节的别名文件（以 `book` 开头），
 * 真正的 bundle 路径以 `file://` URL 的形式记录在文件里。直接读字节即可解析，
 * 不依赖 AppleScript / 自动化授权。
 */

const ALIAS_MAGIC = 'book'

/**
 * 别名记录里的路径字符串以 `\0` 结尾，所以用 `[^\0]` 圈定范围再贪婪匹配到最后一个
 * `.app`：`file:///Applications/Doubao.app/Contents/Helpers/Doubao%20Browser.app` 这种
 * 嵌套路径必须取完整路径，不能停在中间的 `.app` 上。
 */
const TARGET_PATTERN = /file:\/\/\/[^\0]+\.app/g

/** 从别名文件的字节里取出所有可能的目标路径（URL 解码、去重、保持顺序）。 */
export function aliasTargetsFromBytes(raw: Buffer): string[] {
  if (raw.subarray(0, ALIAS_MAGIC.length).toString('latin1') !== ALIAS_MAGIC) return []
  const text = raw.toString('latin1')
  const targets: string[] = []
  for (const match of text.matchAll(TARGET_PATTERN)) {
    try {
      const target = decodeURIComponent(match[0].slice('file://'.length))
      if (!targets.includes(target)) targets.push(target)
    } catch {
      // 不是合法的 URL 编码，跳过这个候选。
    }
  }
  return targets
}
