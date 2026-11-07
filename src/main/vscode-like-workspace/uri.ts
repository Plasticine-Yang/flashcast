import { sep, win32 } from 'node:path'
import type { EditorWorkspaceRemote, PlatformId } from '@shared/types'

export interface ParsedWorkspaceUri {
  uri: string
  path: string
  kind: 'folder' | 'workspace'
  remote: EditorWorkspaceRemote | null
  /** Stable de-duplication key within one editor variant. */
  key: string
}

function decode(value: string): string {
  try {
    return decodeURIComponent(value)
  } catch {
    return value
  }
}

function isWorkspaceFile(path: string): boolean {
  return /\.code-workspace$/i.test(path)
}

function workspaceName(path: string): string {
  const name = path.split(/[\\/]/).filter(Boolean).at(-1) ?? path
  return isWorkspaceFile(name) ? name.replace(/\.code-workspace$/i, '') : name
}

function remoteInfo(authority: string): EditorWorkspaceRemote {
  const decoded = decode(authority)
  const separator = decoded.indexOf('+')
  const type = separator < 0 ? decoded : decoded.slice(0, separator)
  const host = separator < 0 ? decoded : decoded.slice(separator + 1)
  return {
    authority: decoded,
    host: host || type,
    label: `${type} · ${host || type}`
  }
}

/**
 * Parses a VS Code workspace URI (`file:` or `vscode-remote:`) into the local
 * path / remote authority the launcher can display and open. Returns `null` for
 * unsupported schemes or malformed URIs.
 */
export function parseWorkspaceUri(
  raw: string,
  platform: PlatformId,
  kindHint: 'folder' | 'workspace' = 'folder'
): ParsedWorkspaceUri | null {
  if (!raw) return null
  let url: URL
  try {
    url = new URL(raw)
  } catch {
    return null
  }

  if (url.protocol === 'file:') {
    let path = decode(url.pathname)
    if (platform === 'win32') {
      if (url.hostname) path = `\\\\${url.hostname}${path.replace(/\//g, '\\')}`
      else if (/^\/[a-zA-Z]:/.test(path)) path = path.slice(1).replace(/\//g, '\\')
    }
    if (!path) return null
    const kind = isWorkspaceFile(path) ? 'workspace' : kindHint
    return {
      uri: raw,
      path,
      kind,
      remote: null,
      key: `${platform === 'win32' ? path.toLowerCase() : path}`
    }
  }

  if (url.protocol === 'vscode-remote:') {
    const path = decode(url.pathname)
    if (!path) return null
    const remote = remoteInfo(url.hostname)
    return {
      uri: raw,
      path,
      kind: isWorkspaceFile(path) ? 'workspace' : kindHint,
      remote,
      key: `${remote.authority}${path}`
    }
  }

  return null
}

export function workspaceDisplayPath(
  path: string,
  remote: EditorWorkspaceRemote | null,
  home: string,
  platform: PlatformId
): string {
  if (remote) return `${remote.host}:${path}`
  const separator = platform === 'win32' ? win32.sep : sep
  if (home && (path === home || path.startsWith(`${home}${separator}`))) {
    return `~${path.slice(home.length)}`
  }
  return path
}

export function workspaceNameFor(path: string, remote: EditorWorkspaceRemote | null): string {
  return workspaceName(path)
}
