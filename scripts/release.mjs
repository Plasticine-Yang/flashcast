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
 *
 * `--watch` 会在推送后跟踪对应的 Actions run，成功时打印 Release 地址，
 * 失败时打印排查命令（需要已登录的 gh CLI）：
 *   pnpm release:patch -- --watch
 *
 * 失败处置见 docs/release.md。
 */
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const packagePath = join(root, 'package.json')
const SEMVER = /^(\d+)\.(\d+)\.(\d+)$/
const cliArgs = process.argv.slice(2)
const watch = cliArgs.includes('--watch')
const kind = (cliArgs.find((arg) => arg !== '--watch') ?? 'minor').trim()

function run(command, args, { quiet = false } = {}) {
  return execFileSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', quiet ? 'pipe' : 'pipe', 'inherit']
  }).trim()
}

function runLive(command, args) {
  execFileSync(command, args, { cwd: root, stdio: 'inherit' })
}

function ghJson(args) {
  return JSON.parse(run('gh', args, { quiet: true }))
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

if (!watch) {
  console.log('GitHub Actions 正在构建三端产物并创建 Release，进度见仓库 Actions 页面。')
  console.log('想在这里等到结果：pnpm release:patch -- --watch')
  process.exit(0)
}

/** tag 推送后 run 会晚几秒出现，轮询到匹配 headBranch 的那条。 */
async function findRun() {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    try {
      const runs = ghJson([
        'run',
        'list',
        '--workflow',
        'release.yml',
        '--limit',
        '20',
        '--json',
        'databaseId,headBranch'
      ])
      const found = runs.find((entry) => entry.headBranch === tag)
      if (found) return found.databaseId
    } catch (error) {
      if (error.code === 'ENOENT') {
        console.error('release: 未找到 gh CLI，--watch 需要先安装并登录 gh。')
        process.exit(1)
      }
      // gh 未登录或接口抖动：继续等，超时后由调用方给出提示。
    }
    await sleep(2000)
  }
  return null
}

const runId = await findRun()
if (!runId) {
  console.error('release: 未找到对应的 Actions run。确认 gh 已登录后，到 Actions 页面手动查看。')
  process.exit(1)
}

console.log(`\n跟踪 Release run ${runId}（Ctrl-C 只停止跟踪，不会中断流水线）…`)
let watched = false
try {
  runLive('gh', ['run', 'watch', String(runId), '--exit-status', '--interval', '20'])
  watched = true
} catch {
  watched = false
}

let conclusion = ''
let runUrl = ''
try {
  const view = ghJson(['run', 'view', String(runId), '--json', 'conclusion,url'])
  conclusion = String(view.conclusion ?? '')
  runUrl = String(view.url ?? '')
} catch {
  // 拿不到详情时只保留下面的通用提示。
}

if (watched && conclusion === 'success') {
  const url = run('gh', ['release', 'view', tag, '--json', 'url', '--jq', '.url'], { quiet: true })
  console.log(`\nRelease 已发布：${url}`)
  process.exit(0)
}

console.error(`\nrelease: 流水线未通过${conclusion ? `（${conclusion}）` : ''}${runUrl ? `：${runUrl}` : ''}`)
console.error(`  查看失败日志：gh run view ${runId} --log-failed`)
console.error(`  修复后重试：  gh run rerun ${runId} --failed`)
console.error('  已知瞬时故障（GitHub 资产 CDN 504）等几分钟直接重跑即可；详见 docs/release.md')
process.exit(1)
