import { getAcpModelConfigOption } from '@/lib/pulse/acp-config'
import type { AcpConfigOption } from '@/lib/pulse/types'
import { VALID_AGENTS, VALID_PERMISSIONS, validateStoredEnum } from './storage'
import type { PulseWorkspaceAgent, PulseWorkspaceModel, PulseWorkspacePermission } from './types'

export function resolveStoredPulseState(input: {
  storedMode: string | null
  storedAgent: string | null
  storedModel: string | null
  storedPermission: string | null
}): {
  workspaceMode: string | null
  pulseAgent: PulseWorkspaceAgent
  pulseModel: PulseWorkspaceModel | null
  pulsePermissionLevel: PulseWorkspacePermission
} {
  return {
    workspaceMode: input.storedMode || null,
    pulseAgent: validateStoredEnum(
      input.storedAgent,
      VALID_AGENTS,
      'claude' as PulseWorkspaceAgent,
    ),
    pulseModel: input.storedModel && input.storedModel.length > 0 ? input.storedModel : null,
    pulsePermissionLevel: validateStoredEnum(
      input.storedPermission,
      VALID_PERMISSIONS,
      'accept-edits' as PulseWorkspacePermission,
    ),
  }
}

export function resolvePulseProbeResult(
  options: AcpConfigOption[],
  pulseModel: PulseWorkspaceModel,
): {
  acpConfigOptions: AcpConfigOption[]
  nextPulseModel: PulseWorkspaceModel | null
} {
  if (options.length === 0) {
    return {
      acpConfigOptions: options,
      nextPulseModel: null,
    }
  }

  const modelConfig = getAcpModelConfigOption(options)
  if (!modelConfig || modelConfig.options.length === 0) {
    return {
      acpConfigOptions: options,
      nextPulseModel: null,
    }
  }

  const hasCurrent = modelConfig.options.some((option) => option.value === pulseModel)
  if (hasCurrent) {
    return {
      acpConfigOptions: options,
      nextPulseModel: null,
    }
  }

  return {
    acpConfigOptions: options,
    nextPulseModel: (modelConfig.currentValue ||
      modelConfig.options[0]!.value) as PulseWorkspaceModel,
  }
}
