import { AlertTriangle, FolderOpen, GitBranch, GitCommitHorizontal, RefreshCw, RotateCcw } from 'lucide-react'
import { useState } from 'react'
import type { BootstrapData, GitDiffResult } from '@shared/types'
import { formatDateTime, relativeTime } from '@shared/time'
import { api } from '../bridge'
import { InlineSpinner, Modal, PageHeader, Row, Section, SyncChip } from './components'

type ToastTone = 'default' | 'success' | 'warning' | 'danger'

export function WorkspacePage({
  data,
  onData,
  onToast
}: {
  data: BootstrapData
  onData: (data: BootstrapData) => void
  onToast: (message: string, tone?: ToastTone) => void
}): React.JSX.Element {
  const [busy, setBusy] = useState(false)
  const [choosing, setChoosing] = useState(false)
  const [diffOpen, setDiffOpen] = useState(false)
  const [diff, setDiff] = useState<GitDiffResult | null>(null)
  const [diffLoading, setDiffLoading] = useState(false)
  const [autoCommitBusy, setAutoCommitBusy] = useState(false)
  const workspace = data.workspace

  const refresh = async (): Promise<void> => {
    setBusy(true)
    try {
      const summary = await api.refreshWorkspace()
      onData({ ...data, workspace: summary })
      onToast('工作区状态已刷新', 'success')
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setBusy(false)
    }
  }

  const choose = async (): Promise<void> => {
    setChoosing(true)
    try {
      const result = await api.chooseWorkspace()
      if (result.canceled) return
      const next = await api.getBootstrap()
      onData(next)
      onToast('已切换工作区', 'success')
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setChoosing(false)
    }
  }

  const toggleAutoCommit = async (): Promise<void> => {
    setAutoCommitBusy(true)
    try {
      const next = await api.updatePrefs({ autoCommit: !workspace.autoCommit })
      onData(next)
      onToast(workspace.autoCommit ? '已关闭自动提交' : '已开启自动提交并推送', 'success')
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setAutoCommitBusy(false)
    }
  }

  const openDiff = async (): Promise<void> => {
    setDiffOpen(true)
    setDiffLoading(true)
    setDiff(null)
    try {
      const result = await api.getGitDiff()
      setDiff(result)
    } catch (error) {
      setDiff({ status: '', diff: '', error: (error as Error).message })
    } finally {
      setDiffLoading(false)
    }
  }

  return (
    <>
      <PageHeader
        title="工作区"
        description="工作区是一个 Git 仓库，集中保存 Flashcast 配置与每个插件的设置。"
        actions={
          <button type="button" className="settings-ghost-button" onClick={() => void refresh()} disabled={busy}>
            <RefreshCw size={13} strokeWidth={1.75} aria-hidden="true" />
            刷新状态
          </button>
        }
      />
      <div className="settings-stack">
        <div className="settings-workspace-card">
          <div className="settings-workspace-head">
            <div className="settings-workspace-icon">
              <GitBranch size={20} strokeWidth={1.5} aria-hidden="true" />
            </div>
            <div className="settings-workspace-path">
              <span className="settings-row-label">当前仓库</span>
              <code>{workspace.path}</code>
            </div>
            <SyncChip state={workspace.syncState} />
          </div>

          <div className="settings-workspace-stats">
            <div className="settings-stat">
              <span className="settings-stat-label">分支</span>
              <span className="settings-stat-value">{workspace.branch ?? '—'}</span>
            </div>
            <div className="settings-stat">
              <span className="settings-stat-label">最近提交</span>
              <span className="settings-stat-value">
                {workspace.lastCommit ? (
                  <>
                    <GitCommitHorizontal size={12} strokeWidth={1.75} aria-hidden="true" />
                    {workspace.lastCommit.message}
                  </>
                ) : (
                  '尚无提交'
                )}
              </span>
              <span className="settings-stat-hint">
                {workspace.lastCommit
                  ? `${workspace.lastCommit.hash} · ${relativeTime(workspace.lastCommit.date)}`
                  : '开启自动提交或手动提交后会显示'}
              </span>
            </div>
            <div className="settings-stat">
              <span className="settings-stat-label">未提交改动</span>
              <span className="settings-stat-value tnum">{workspace.dirtyCount} 个文件</span>
              <span className="settings-stat-hint">
                {workspace.ahead > 0 ? `${workspace.ahead} 个提交待推送` : '本地已包含全部提交'}
                {workspace.behind > 0 ? ` · 落后远程 ${workspace.behind}` : ''}
              </span>
            </div>
            <div className="settings-stat">
              <span className="settings-stat-label">远程仓库</span>
              <span className="settings-stat-value">{workspace.hasRemote ? '已配置' : '未配置'}</span>
              <span className="settings-stat-hint">
                {workspace.syncState === 'conflict'
                  ? '请解决冲突后自动提交才能继续'
                  : workspace.hasRemote
                    ? '自动提交会尝试推送当前分支'
                    : '配置远程与 upstream 后可自动推送'}
              </span>
            </div>
          </div>

          {workspace.lastError ? (
            <div className="settings-warning">
              <AlertTriangle size={14} strokeWidth={1.75} aria-hidden="true" />
              <span>{workspace.lastError}</span>
            </div>
          ) : null}

          <div className="settings-workspace-actions">
            <button
              type="button"
              className="fc-btn"
              onClick={async () => {
                try {
                  await api.openWorkspaceFolder()
                } catch (error) {
                  onToast((error as Error).message, 'danger')
                }
              }}
            >
              <FolderOpen size={13} strokeWidth={1.75} aria-hidden="true" />
              在文件管理器中打开
            </button>
            <button type="button" className="fc-btn" onClick={() => void openDiff()}>
              查看差异
            </button>
            {workspace.conflict ? (
              <button
                type="button"
                className="fc-btn fc-btn--danger"
                onClick={() => void openDiff()}
              >
                <AlertTriangle size={13} strokeWidth={1.75} aria-hidden="true" />
                处理冲突
              </button>
            ) : null}
            <button
              type="button"
              className="fc-btn"
              onClick={() => void choose()}
              disabled={choosing}
            >
              {choosing ? <span className="fc-spinner" aria-hidden="true" /> : <RotateCcw size={13} strokeWidth={1.75} aria-hidden="true" />}
              重新关联仓库
            </button>
          </div>
        </div>

        <Section
          title="自动提交并推送"
          description="只提交 Flashcast 管理的 config.json、plugins/、themes/、extensions/、README.md 与 .gitignore，不会改动仓库中的其他文件。"
        >
          <Row
            label="自动提交并推送"
            description="配置变更后约 1 秒内提交；若分支配置了 upstream 则尝试推送。默认关闭。"
          >
            <span className="settings-row-inline">
              {autoCommitBusy ? <InlineSpinner /> : null}
              <button
                type="button"
                className="fc-switch"
                role="switch"
                aria-checked={workspace.autoCommit}
                aria-label="自动提交并推送"
                disabled={autoCommitBusy || !workspace.isRepo}
                onClick={() => void toggleAutoCommit()}
              />
            </span>
          </Row>
        </Section>

        <Section
          title="工作区文件"
          description="state/ 是机器本地目录并已被忽略；themes/ 与 extensions/ 会纳入版本控制并自动提交。"
        >
          <div className="settings-kv settings-kv--card">
            <span>数据目录</span>
            <code>{data.dataDir}</code>
          </div>
          <div className="settings-kv settings-kv--card">
            <span>最近同步</span>
            <span>
              {workspace.lastCommit ? formatDateTime(workspace.lastCommit.date) : '尚未同步'}
            </span>
          </div>
          <p className="settings-hint">
            删除或移动工作区前请先关闭 Flashcast。重新关联后，主题与插件会从仓库中恢复，快捷键等设置会从新工作区加载。
          </p>
        </Section>
      </div>

      <Modal
        open={diffOpen}
        title="工作区差异"
        description="展示 Git 状态与已暂存 / 未暂存的改动摘要。"
        onClose={() => setDiffOpen(false)}
        width={700}
        footer={
          <button type="button" className="fc-btn" onClick={() => setDiffOpen(false)}>
            关闭
          </button>
        }
      >
        {diffLoading ? (
          <InlineSpinner label="正在读取 git diff…" />
        ) : diff?.error ? (
          <div className="settings-form-error">
            <AlertTriangle size={13} strokeWidth={2} aria-hidden="true" />
            {diff.error}
          </div>
        ) : (
          <div className="settings-diff">
            <div>
              <div className="settings-field-label">git status --short</div>
              <pre>{diff?.status || '工作区干净，没有未提交的改动。'}</pre>
            </div>
            <div>
              <div className="settings-field-label">git diff --stat</div>
              <pre>{diff?.diff || '没有文件级差异。'}</pre>
            </div>
          </div>
        )}
      </Modal>
    </>
  )
}
