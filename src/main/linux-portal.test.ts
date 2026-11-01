import { describe, expect, it } from 'vitest'
import { desktopEntryDirs, findDesktopEntry, packageDesktopName, usesWayland } from './linux-portal'

describe('usesWayland', () => {
  it('follows the session type when both displays exist', () => {
    expect(usesWayland({ XDG_SESSION_TYPE: 'wayland', WAYLAND_DISPLAY: 'wayland-0', DISPLAY: ':0' })).toBe(true)
    expect(usesWayland({ XDG_SESSION_TYPE: 'x11', WAYLAND_DISPLAY: 'wayland-0', DISPLAY: ':0' })).toBe(false)
  })

  it('breaks ties on the available display', () => {
    expect(usesWayland({ WAYLAND_DISPLAY: 'wayland-0' })).toBe(true)
    expect(usesWayland({ DISPLAY: ':0' })).toBe(false)
  })
})

describe('desktopEntryDirs', () => {
  it('prefers XDG_DATA_HOME and keeps XDG_DATA_DIRS order', () => {
    expect(
      desktopEntryDirs(
        { XDG_DATA_HOME: '/data/home', XDG_DATA_DIRS: '/usr/local/share:/usr/share' },
        '/home/test'
      )
    ).toEqual(['/data/home', '/usr/local/share', '/usr/share'])
  })

  it('falls back to the per-user applications directory', () => {
    expect(desktopEntryDirs({ XDG_DATA_DIRS: '/usr/share' }, '/home/test')).toEqual([
      '/home/test/.local/share',
      '/usr/share'
    ])
  })
})

describe('findDesktopEntry', () => {
  it('returns the first matching applications entry', () => {
    const exists = (path: string) => path === '/usr/share/applications/com.flashcast.app.desktop'
    expect(
      findDesktopEntry('com.flashcast.app.desktop', ['/home/test/.local/share', '/usr/share'], exists)
    ).toBe('/usr/share/applications/com.flashcast.app.desktop')
  })

  it('returns null when no entry is installed', () => {
    expect(findDesktopEntry('com.flashcast.app.desktop', ['/usr/share'], () => false)).toBeNull()
  })

  it('rejects absolute paths', () => {
    expect(
      findDesktopEntry('/tmp/com.flashcast.app.desktop', ['/usr/share'], () => true)
    ).toBeNull()
  })
})

describe('packageDesktopName', () => {
  it('reads desktopName from package.json', () => {
    expect(packageDesktopName()).toBe('com.flashcast.app.desktop')
  })
})
