import { describe, expect, it } from 'vitest'
import { resolvePulseProbeResult, resolveStoredPulseState } from '@/hooks/ws-messages/pulse'
import type { AcpConfigOption } from '@/lib/pulse/types'

function makeOption(partial: Partial<AcpConfigOption>): AcpConfigOption {
  return {
    id: 'opt',
    name: 'Option',
    currentValue: 'default',
    options: [],
    ...partial,
  }
}

describe('resolveStoredPulseState', () => {
  it('validates agent and permission while preserving truthy workspace mode and model', () => {
    expect(
      resolveStoredPulseState({
        storedMode: 'research',
        storedAgent: 'not-real',
        storedModel: 'opus',
        storedPermission: 'invalid',
      }),
    ).toEqual({
      workspaceMode: 'research',
      pulseAgent: 'claude',
      pulseModel: 'opus',
      pulsePermissionLevel: 'accept-edits',
    })
  })

  it('drops empty model strings and missing workspace mode', () => {
    expect(
      resolveStoredPulseState({
        storedMode: null,
        storedAgent: 'codex',
        storedModel: '',
        storedPermission: 'plan',
      }),
    ).toEqual({
      workspaceMode: null,
      pulseAgent: 'codex',
      pulseModel: null,
      pulsePermissionLevel: 'plan',
    })
  })
})

describe('resolvePulseProbeResult', () => {
  it('keeps the current model when probe options still contain it', () => {
    const options: AcpConfigOption[] = [
      makeOption({
        id: 'model',
        name: 'Model',
        category: 'model',
        currentValue: 'sonnet',
        options: [
          { value: 'sonnet', name: 'Sonnet' },
          { value: 'opus', name: 'Opus' },
        ],
      }),
    ]

    expect(resolvePulseProbeResult(options, 'opus')).toEqual({
      acpConfigOptions: options,
      nextPulseModel: null,
    })
  })

  it('falls back to config currentValue or first option when current model is unavailable', () => {
    const currentValueOptions: AcpConfigOption[] = [
      makeOption({
        id: 'model',
        name: 'Model',
        category: 'model',
        currentValue: 'sonnet',
        options: [
          { value: 'sonnet', name: 'Sonnet' },
          { value: 'opus', name: 'Opus' },
        ],
      }),
    ]
    const firstOptionOnly: AcpConfigOption[] = [
      makeOption({
        id: 'model',
        name: 'Model',
        category: 'model',
        currentValue: '',
        options: [
          { value: 'haiku', name: 'Haiku' },
          { value: 'sonnet', name: 'Sonnet' },
        ],
      }),
    ]

    expect(resolvePulseProbeResult(currentValueOptions, 'missing')).toEqual({
      acpConfigOptions: currentValueOptions,
      nextPulseModel: 'sonnet',
    })
    expect(resolvePulseProbeResult(firstOptionOnly, 'missing')).toEqual({
      acpConfigOptions: firstOptionOnly,
      nextPulseModel: 'haiku',
    })
  })
})
