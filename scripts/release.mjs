#!/usr/bin/env node
/**
 * 发布脚本：升版本 → 提交 → 打 tag → 推送。
 *
 * 推送 `v*` tag 会触发 .github/workflows/release.yml：
 * 三端构建（Linux AppImage/deb、macOS dmg/zip、Windows nsis）后创建 GitHub Release。
 *
 * 用法：
 *   pnpm release:minor      # 0.1.0 → 0.2.0
 *   pnpm release:patch      # 0.1.0 → 0.1.1
 *   pnpm release:major      # 0.1.0 → 1.0.0
 *   node scripts/release.mjs 1.4.0   # 指定版本
 */
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const packagePath = join(root, 'package.json')
const SEMVER = /^(\d+)\.(\d+)\.(\d+)$/
const kind = (process.argv[2] ?? 'minor').trim()

function run(command, args, { quiet = false } = {}) {
  return execFileSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', quiet ? 'pipe' : 'pipe', 'inherit']
  }).trim()
}

function fail(message) {
  console.error(`release: ${message}`)
  process.exit(1)
}

if (run('git', ['status', '--porcelain'])) {
  fail('工作区有未提交的改动，先提交后再发布')
}

const branch = run('git', ['rev-parse', '--abbrev-ref', 'HEAD'])
if (branch !== 'main') fail(`当前在 ${branch} 分支，发布前请切回 main`)

const text = readFileSync(packagePath, 'utf8')
const current = String(JSON.parse(text).version)
const match = SEMVER.exec(current)
if (!match) fail(`package.json 版本 ${current} 不是 x.y.z 形式`)

let next
if (kind === 'major' || kind === 'minor' || kind === 'patch') {
  const [major, minor, patch] = match.slice(1).map(Number)
  if (kind === 'major') next = `${major + 1}.0.0`
  else if (kind === 'minor') next = `${major}.${minor + 1}.0`
  else next = `${major}.${minor}.${patch + 1}`
} else if (SEMVER.test(kind)) {
  next = kind
} else {
  fail(`无法识别的版本参数 ${kind}（可用 major / minor / patch / x.y.z）`)
}

const tag = `v${next}`
try {
  run('git', ['rev-parse', '--verify', '--quiet', `refs/tags/${tag}`], { quiet: true })
  fail(`tag ${tag} 已存在`)
} catch {
  // 不存在才能继续。
}

writeFileSync(packagePath, text.replace(/("version":\s*")[^"]+(")/, `$1${next}$2`))

run('git', ['add', 'package.json'])
run('git', ['commit', '-m', `chore(release): 发布 ${tag}`])
run('git', ['tag', '-a', tag, '-m', `Flashcast ${next}`])
run('git', ['push', 'origin', 'main'])
run('git', ['push', 'origin', tag])

console.log(`\n已推送 ${tag}（${current} → ${next}）。`)
console.log('GitHub Actions 正在构建三端产物并创建 Release，进度见仓库 Actions 页面。')
