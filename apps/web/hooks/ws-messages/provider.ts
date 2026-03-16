'use client'

import { usePathname } from 'next/navigation'
import { useMemo, useRef } from 'react'
import { useAxonWs } from '@/hooks/use-axon-ws'
import type { AcpConfigOption } from '@/lib/pulse/types'
import { usePulseSlice, useWorkspaceSlice } from '@/lib/shell-store'
import type { MessageHandlerSetters } from './handlers'
import { useWsProviderActions } from './provider-actions'
import {
  createSetStateActionBridge,
  usePersistedPulseState,
  usePulseConfigProbe,
  useRuntimeSubscription,
  useStoredPulseHydration,
} from './provider-effects'
import { useWsProviderRuntime } from './provider-runtime'
import type {
  PulseWorkspaceAgent,
  PulseWorkspaceModel,
  PulseWorkspacePermission,
  WsMessagesContextValue,
  WsMessagesExecutionState,
  WsMessagesWorkspaceState,
} from './types'

export function useWsMessagesProvider() {
  const pathname = usePathname()
  const { subscribeByTypes, send } = useAxonWs()
  const runtime = useWsProviderRuntime()

  const {
    pulseAgent,
    pulseModel,
    pulsePermissionLevel,
    acpConfigOptions,
    setPulseAgent,
    setPulseModel,
    setPulsePermissionLevel,
    setAcpConfigOptions,
  } = usePulseSlice() as {
    pulseAgent: PulseWorkspaceAgent
    pulseModel: PulseWorkspaceModel
    pulsePermissionLevel: PulseWorkspacePermission
    acpConfigOptions: AcpConfigOption[]
    setPulseAgent: (agent: PulseWorkspaceAgent) => void
    setPulseModel: (model: PulseWorkspaceModel) => void
    setPulsePermissionLevel: (level: PulseWorkspacePermission) => void
    setAcpConfigOptions: (options: AcpConfigOption[]) => void
  }

  const {
    workspaceMode,
    workspacePrompt,
    workspacePromptVersion,
    workspaceResumeSessionId,
    workspaceResumeVersion,
    workspaceContext,
    setWorkspaceMode,
    setWorkspacePrompt,
    setWorkspacePromptVersion,
    bumpWorkspacePromptVersion,
    setWorkspaceResumeSessionId,
    setWorkspaceResumeVersion,
    bumpWorkspaceResumeVersion,
    setWorkspaceContext,
  } = useWorkspaceSlice()

  useStoredPulseHydration({
    setWorkspaceMode,
    setPulseAgent,
    setPulseModel,
    setPulsePermissionLevel,
  })

  usePersistedPulseState({
    workspaceMode,
    pulseAgent,
    pulseModel,
    pulsePermissionLevel,
  })

  usePulseConfigProbe({
    pathname,
    pulseAgent,
    pulseModel,
    setAcpConfigOptions,
    setPulseModel,
  })

  // Keep getter refs up to date so the bridge closures always read current values
  // without needing to recreate the bridge function on every render.
  const workspaceModeRef = useRef(workspaceMode)
  workspaceModeRef.current = workspaceMode
  const workspacePromptRef = useRef(workspacePrompt)
  workspacePromptRef.current = workspacePrompt
  const workspacePromptVersionRef = useRef(workspacePromptVersion)
  workspacePromptVersionRef.current = workspacePromptVersion

  const setWorkspaceModeBridge = useMemo(
    () => createSetStateActionBridge(setWorkspaceMode, () => workspaceModeRef.current),
    [setWorkspaceMode],
  )
  const setWorkspacePromptBridge = useMemo(
    () => createSetStateActionBridge(setWorkspacePrompt, () => workspacePromptRef.current),
    [setWorkspacePrompt],
  )
  const setWorkspacePromptVersionBridge = useMemo(
    () =>
      createSetStateActionBridge(
        setWorkspacePromptVersion,
        () => workspacePromptVersionRef.current,
      ),
    [setWorkspacePromptVersion],
  )

  const subscriptionSetters = useMemo<MessageHandlerSetters>(
    () => ({
      setLogLines: runtime.setters.setLogLines,
      setMarkdownContent: runtime.setters.setMarkdownContent,
      setHasResults: runtime.setters.setHasResults,
      setCrawlFiles: runtime.setters.setCrawlFilesTracked,
      setCurrentOutputDir: runtime.setters.setCurrentOutputDirTracked,
      setSelectedFile: runtime.setters.setSelectedFileTracked,
      setCrawlProgress: runtime.setters.setCrawlProgress,
      setCommandMode: runtime.setters.setCommandMode,
      setStdoutLines: runtime.setters.setStdoutLines,
      setStdoutJson: runtime.setters.setStdoutJsonTracked,
      setVirtualFileContentByPath: runtime.setters.setVirtualFileContentByPathTracked,
      setScreenshotFiles: runtime.setters.setScreenshotFiles,
      setLifecycleEntries: runtime.setters.setLifecycleEntries,
      setCancelResponse: runtime.setters.setCancelResponse,
      setIsProcessing: runtime.setters.setIsProcessing,
      setErrorMessage: runtime.setters.setErrorMessage,
      setRecentRuns: runtime.setters.setRecentRuns,
      setWorkspaceMode: setWorkspaceModeBridge,
      setWorkspacePrompt: setWorkspacePromptBridge,
      setWorkspacePromptVersion: setWorkspacePromptVersionBridge,
      setCurrentJobIdTracked: runtime.setters.setCurrentJobIdTracked,
    }),
    [
      runtime.setters,
      setWorkspaceModeBridge,
      setWorkspacePromptBridge,
      setWorkspacePromptVersionBridge,
    ],
  )

  useRuntimeSubscription({
    subscribeByTypes,
    refs: runtime.refs,
    setters: subscriptionSetters,
  })

  const actions = useWsProviderActions({
    send,
    currentModeRef: runtime.refs.currentModeRef,
    currentInputRef: runtime.refs.currentInputRef,
    virtualFileContentByPathRef: runtime.refs.virtualFileContentByPathRef,
    runtimeStateRef: runtime.refs.runtimeStateRef,
    setMarkdownContent: runtime.setters.setMarkdownContent,
    setLogLines: runtime.setters.setLogLines,
    setErrorMessage: runtime.setters.setErrorMessage,
    setIsProcessing: runtime.setters.setIsProcessing,
    setHasResults: runtime.setters.setHasResults,
    setCurrentMode: runtime.setters.setCurrentMode,
    setCrawlFilesTracked: runtime.setters.setCrawlFilesTracked,
    setSelectedFileTracked: runtime.setters.setSelectedFileTracked,
    setCrawlProgress: runtime.setters.setCrawlProgress,
    setStdoutLines: runtime.setters.setStdoutLines,
    setStdoutJsonTracked: runtime.setters.setStdoutJsonTracked,
    setCommandMode: runtime.setters.setCommandMode,
    setScreenshotFiles: runtime.setters.setScreenshotFiles,
    setCurrentJobIdTracked: runtime.setters.setCurrentJobIdTracked,
    setLifecycleEntries: runtime.setters.setLifecycleEntries,
    setCancelResponse: runtime.setters.setCancelResponse,
    setCurrentOutputDirTracked: runtime.setters.setCurrentOutputDirTracked,
    setVirtualFileContentByPathTracked: runtime.setters.setVirtualFileContentByPathTracked,
    setWorkspaceMode,
    setWorkspacePrompt,
    setWorkspacePromptVersion,
    bumpWorkspacePromptVersion,
    setWorkspaceResumeSessionId,
    setWorkspaceResumeVersion,
    bumpWorkspaceResumeVersion,
    setWorkspaceContext,
    setPulseAgent,
    setPulseModel,
    setPulsePermissionLevel,
    setAcpConfigOptions,
  })

  const executionState = useMemo<WsMessagesExecutionState>(
    () => ({ ...runtime.state }),
    [runtime.state],
  )

  const workspaceState = useMemo<WsMessagesWorkspaceState>(
    () => ({
      workspaceMode,
      workspacePrompt,
      workspacePromptVersion,
      workspaceResumeSessionId,
      workspaceResumeVersion,
      workspaceContext,
      pulseAgent,
      pulseModel,
      pulsePermissionLevel,
      acpConfigOptions,
    }),
    [
      workspaceMode,
      workspacePrompt,
      workspacePromptVersion,
      workspaceResumeSessionId,
      workspaceResumeVersion,
      workspaceContext,
      pulseAgent,
      pulseModel,
      pulsePermissionLevel,
      acpConfigOptions,
    ],
  )

  const value = useMemo<WsMessagesContextValue>(
    () => ({
      ...executionState,
      ...workspaceState,
      ...actions,
    }),
    [executionState, workspaceState, actions],
  )

  return { executionState, workspaceState, actions, value }
}
