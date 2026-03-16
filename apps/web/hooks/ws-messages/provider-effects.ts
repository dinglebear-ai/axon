import type { Dispatch, SetStateAction } from 'react'
import { useEffect } from 'react'
import { probePulseConfigOptions } from '@/lib/pulse/config-api'
import type { AcpConfigOption } from '@/lib/pulse/types'
import type { WsServerMsg } from '@/lib/ws-protocol'
import type { MessageHandlerRefs, MessageHandlerSetters } from './handlers'
import { resolvePulseProbeResult, resolveStoredPulseState } from './pulse'
import {
  LS_PULSE_AGENT,
  LS_PULSE_MODEL,
  LS_PULSE_PERMISSION,
  LS_WORKSPACE_MODE,
  safeGetItem,
  safeRemoveItem,
  safeSetItem,
} from './storage'
import { subscribeRuntimeMessages } from './subscription'
import type { PulseWorkspaceAgent, PulseWorkspaceModel, PulseWorkspacePermission } from './types'

export function createSetStateActionBridge<T>(
  setValue: (value: T) => void,
  getValue: () => T,
): Dispatch<SetStateAction<T>> {
  return (action) => {
    if (typeof action === 'function') {
      setValue((action as (prev: T) => T)(getValue()))
      return
    }
    setValue(action)
  }
}

export function useStoredPulseHydration(input: {
  setWorkspaceMode: (mode: string | null) => void
  setPulseAgent: (agent: PulseWorkspaceAgent) => void
  setPulseModel: (model: PulseWorkspaceModel) => void
  setPulsePermissionLevel: (level: PulseWorkspacePermission) => void
}) {
  useEffect(() => {
    const stored = resolveStoredPulseState({
      storedMode: safeGetItem(LS_WORKSPACE_MODE),
      storedAgent: safeGetItem(LS_PULSE_AGENT),
      storedModel: safeGetItem(LS_PULSE_MODEL),
      storedPermission: safeGetItem(LS_PULSE_PERMISSION),
    })
    if (stored.workspaceMode) input.setWorkspaceMode(stored.workspaceMode)
    input.setPulseAgent(stored.pulseAgent)
    if (stored.pulseModel) input.setPulseModel(stored.pulseModel)
    input.setPulsePermissionLevel(stored.pulsePermissionLevel)
  }, [
    input.setPulseAgent,
    input.setPulseModel,
    input.setPulsePermissionLevel,
    input.setWorkspaceMode,
  ])
}

export function usePersistedPulseState(input: {
  workspaceMode: string | null
  pulseAgent: PulseWorkspaceAgent
  pulseModel: PulseWorkspaceModel
  pulsePermissionLevel: PulseWorkspacePermission
}) {
  useEffect(() => {
    if (input.workspaceMode === null) {
      safeRemoveItem(LS_WORKSPACE_MODE)
    } else {
      safeSetItem(LS_WORKSPACE_MODE, input.workspaceMode)
    }
    safeSetItem(LS_PULSE_AGENT, input.pulseAgent)
    safeSetItem(LS_PULSE_MODEL, input.pulseModel ?? '')
    safeSetItem(LS_PULSE_PERMISSION, input.pulsePermissionLevel)
  }, [input.workspaceMode, input.pulseAgent, input.pulseModel, input.pulsePermissionLevel])
}

export function usePulseConfigProbe(input: {
  pathname: string
  pulseAgent: PulseWorkspaceAgent
  pulseModel: PulseWorkspaceModel
  setAcpConfigOptions: (options: AcpConfigOption[]) => void
  setPulseModel: (model: PulseWorkspaceModel) => void
}) {
  useEffect(() => {
    let cancelled = false

    void probePulseConfigOptions({ agent: input.pulseAgent })
      .then((options) => {
        if (cancelled) return
        const resolved = resolvePulseProbeResult(options, input.pulseModel)
        input.setAcpConfigOptions(resolved.acpConfigOptions)
        if (resolved.nextPulseModel) input.setPulseModel(resolved.nextPulseModel)
      })
      .catch((error: unknown) => {
        if (cancelled) return
        console.warn('[pulse] config probe failed', error)
        input.setAcpConfigOptions([])
      })

    return () => {
      cancelled = true
    }
  }, [input.pulseAgent, input.pulseModel, input.setAcpConfigOptions, input.setPulseModel])
}

export function useRuntimeSubscription(input: {
  subscribeByTypes: (
    types: ReadonlyArray<WsServerMsg['type']>,
    handler: (msg: WsServerMsg) => void,
  ) => () => void
  refs: MessageHandlerRefs
  setters: MessageHandlerSetters
}) {
  useEffect(() => {
    return subscribeRuntimeMessages({
      subscribeByTypes: input.subscribeByTypes,
      refs: input.refs,
      setters: input.setters,
    })
  }, [input.subscribeByTypes, input.refs, input.setters])
}
