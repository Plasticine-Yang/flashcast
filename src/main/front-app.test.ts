import { describe, expect, it } from 'vitest'
import { parseFrontAsn, parseFrontName } from './front-app'

describe('front app sampling', () => {
  it('reads the ASN from `lsappinfo front`', () => {
    expect(parseFrontAsn('ASN:0x0-0x5f05f:\n')).toBe('ASN:0x0-0x5f05f:')
    expect(parseFrontAsn('')).toBeNull()
  })

  it('reads the process name from `lsappinfo info -only name`', () => {
    expect(parseFrontName('"飞书" ASN:0x0-0x5f05f: (in front)')).toBe('飞书')
    expect(parseFrontName('"Visual Studio Code" ASN:0x0-0x1d3:')).toBe('Visual Studio Code')
    expect(parseFrontName('[ NULL ]  ASN:0x0-0x1d3: (in front)')).toBeNull()
  })
})
