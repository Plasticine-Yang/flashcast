import { describe, expect, it } from 'vitest'
import {
  checksumFor,
  installKindFor,
  isNewerVersion,
  normalizeArtifactName,
  normalizeReleaseNotes,
  parseChecksums,
  parseVersion,
  pickUpdateAsset,
  type ReleaseAsset
} from './update-assets'

const REPO_NAME = 'Plasticine-Yang/flashcast'
const BASE = `https://github.com/${REPO_NAME}/releases/download/v0.3.0/`

const asset = (name: string, size = 1024): ReleaseAsset => ({ name, size, url: `${BASE}${name}` })

/** The 0.2.3 release asset list, trimmed to the artifacts the updater looks at. */
const ASSETS: ReleaseAsset[] = [
  asset('Flashcast-0.2.3-arm64-mac.zip', 124_633_691),
  asset('Flashcast-0.2.3-arm64-mac.zip.blockmap', 132_480),
  asset('Flashcast-0.2.3-arm64.dmg', 128_903_228),
  asset('Flashcast-0.2.3-mac.zip', 128_729_462),
  asset('Flashcast-0.2.3.dmg', 132_983_516),
  asset('Flashcast-0.2.3.AppImage', 133_141_423),
  asset('Flashcast.Setup.0.2.3.exe', 126_537_247),
  asset('flashcast_0.2.3_amd64.deb', 105_530_424),
  asset('SHA256SUMS.txt', 1140)
]

describe('version comparison', () => {
  it('parses locked and loose semver', () => {
    expect(parseVersion('v0.2.3')).toEqual({ numbers: [0, 2, 3], prerelease: null })
    expect(parseVersion('1.4')).toEqual({ numbers: [1, 4, 0], prerelease: null })
    expect(parseVersion('0.3.0-beta.2')?.prerelease).toBe('beta.2')
    expect(parseVersion('nightly')).toBeNull()
    expect(parseVersion(null)).toBeNull()
  })

  it('only reports strictly newer releases', () => {
    expect(isNewerVersion('0.3.0', '0.2.3')).toBe(true)
    expect(isNewerVersion('0.2.10', '0.2.9')).toBe(true)
    expect(isNewerVersion('1.0.0', '0.99.99')).toBe(true)
    expect(isNewerVersion('0.2.3', '0.2.3')).toBe(false)
    expect(isNewerVersion('0.2.2', '0.2.3')).toBe(false)
    expect(isNewerVersion('garbage', '0.2.3')).toBe(false)
  })

  it('orders pre-releases below the matching release', () => {
    expect(isNewerVersion('0.3.0', '0.3.0-beta.2')).toBe(true)
    expect(isNewerVersion('0.3.0-beta.2', '0.3.0-beta.1')).toBe(true)
    expect(isNewerVersion('0.3.0-beta.1', '0.3.0')).toBe(false)
    expect(isNewerVersion('0.3.0-rc.1', '0.3.0-beta.9')).toBe(true)
  })
})

describe('asset selection', () => {
  it('picks the arch matching macOS zip for in-place installs', () => {
    expect(pickUpdateAsset(ASSETS, 'darwin', 'arm64')?.name).toBe('Flashcast-0.2.3-arm64-mac.zip')
    expect(pickUpdateAsset(ASSETS, 'darwin', 'x64')?.name).toBe('Flashcast-0.2.3-mac.zip')
  })

  it('falls back to the dmg when no zip is published', () => {
    const dmsOnly = ASSETS.filter((entry) => !entry.name.endsWith('.zip'))
    expect(pickUpdateAsset(dmsOnly, 'darwin', 'arm64')?.name).toBe('Flashcast-0.2.3-arm64.dmg')
  })

  it('picks the NSIS installer on Windows and AppImage on Linux', () => {
    expect(pickUpdateAsset(ASSETS, 'win32', 'x64')?.name).toBe('Flashcast.Setup.0.2.3.exe')
    expect(pickUpdateAsset(ASSETS, 'linux', 'x64')?.name).toBe('Flashcast-0.2.3.AppImage')
    const debOnly = ASSETS.filter((entry) => entry.name.endsWith('.deb'))
    expect(pickUpdateAsset(debOnly, 'linux', 'x64')?.name).toBe('flashcast_0.2.3_amd64.deb')
  })

  it('returns null when the release has nothing usable', () => {
    expect(pickUpdateAsset([asset('SHA256SUMS.txt')], 'linux', 'x64')).toBeNull()
  })
})

describe('install strategy', () => {
  it('relaunches for archives it can swap in place', () => {
    expect(installKindFor('Flashcast-0.2.3-arm64-mac.zip', 'darwin')).toBe('relaunch')
    expect(installKindFor('Flashcast.Setup.0.2.3.exe', 'win32')).toBe('relaunch')
    expect(installKindFor('Flashcast-0.2.3.AppImage', 'linux')).toBe('relaunch')
  })

  it('defers to the OS installer for packages it cannot replace', () => {
    expect(installKindFor('Flashcast-0.2.3.dmg', 'darwin')).toBe('manual')
    expect(installKindFor('flashcast_0.2.3_amd64.deb', 'linux')).toBe('manual')
  })
})

describe('checksum parsing', () => {
  it('maps file names to lower-case digests', () => {
    const text = [
      'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  Flashcast-0.2.3.AppImage',
      'BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB *Flashcast-0.2.3-arm64-mac.zip',
      '',
      'not a checksum line'
    ].join('\n')
    const sums = parseChecksums(text)
    expect(sums['Flashcast-0.2.3.AppImage']).toBe('a'.repeat(64))
    expect(sums['Flashcast-0.2.3-arm64-mac.zip']).toBe('b'.repeat(64))
    expect(Object.keys(sums)).toHaveLength(2)
  })
})

describe('checksum lookup', () => {
  // Real shape of the v0.2.3 SHA256SUMS.txt: Windows artifacts carry spaces in
  // the checksum file but dots in the uploaded asset name.
  const text = [
    'c8ad9eb548c79c5149b9adb9a0c112726b974541f60851ebac4db5cbfad451d9  Flashcast Setup 0.2.3.exe',
    '709af3bcfec306ca82026c4ca9f1c6f23cf2fae481fe4addd757e088d6265cbf  Flashcast-0.2.3-arm64-mac.zip'
  ].join('\n')

  it('normalizes separators so GitHub-renamed assets still match', () => {
    expect(normalizeArtifactName('Flashcast Setup 0.2.3.exe')).toBe(
      normalizeArtifactName('Flashcast.Setup.0.2.3.exe')
    )
    expect(checksumFor(text, 'Flashcast.Setup.0.2.3.exe')).toBe(
      'c8ad9eb548c79c5149b9adb9a0c112726b974541f60851ebac4db5cbfad451d9'
    )
    expect(checksumFor(text, 'Flashcast-0.2.3-arm64-mac.zip')).toBe(
      '709af3bcfec306ca82026c4ca9f1c6f23cf2fae481fe4addd757e088d6265cbf'
    )
    expect(checksumFor(text, 'Flashcast-9.9.9.AppImage')).toBeNull()
  })
})

describe('release notes', () => {
  it('trims, normalizes newlines and clamps long notes', () => {
    expect(normalizeReleaseNotes('  - 修复\r\n- 新增  ')).toBe('- 修复\n- 新增')
    expect(normalizeReleaseNotes('   ')).toBeNull()
    expect(normalizeReleaseNotes(null)).toBeNull()
    expect(normalizeReleaseNotes('x'.repeat(50), 10)).toBe(`${'x'.repeat(10)}…`)
  })
})
