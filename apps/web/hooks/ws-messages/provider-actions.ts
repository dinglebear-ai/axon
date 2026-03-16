import type { Dispatch, MutableRefObject, SetStateAction } from 'react'
import { useCallback, useMemo } from 'react'
import type {
  AcpConfigOption,
  PulseAgent,
  PulseModel,
  PulsePermissionLevel,
} from '@/lib/pulse/types'
import type { CrawlFile, WsLifecycleEntry } from '@/lib/ws-protocol'
import {
  applyActivateWorkspace,
  applyClearWorkspaceResumeSession,
  applyDeactivateWorkspace,
  applyResetExecutionRuntime,
  applyResetWorkspaceRuntime,
  applyResumeWorkspaceSession,
  applyStartExecution,
  applySubmitWorkspacePrompt,
} from './actions'
import { makeInitialRuntimeState } from './runtime'
import { LS_WORKSPACE_MODE, safeRemoveItem } from './storage'
import type {
  CancelResponseState,
  CrawlProgress,
  LogLine,
  ScreenshotFile,
  WorkspaceContextState,
  WsMessagesActions,
  WsMessagesRuntimeState,
} from './types'

export interface UseWsProviderActionsInput {
  send: (message: { type: 'read_file'; path: string }) => void
  currentModeRef: MutableRefObject<string>
  currentInputRef: MutableRefObject<string>
  virtualFileContentByPathRef: MutableRefObject<Record<string, string>>
  runtimeStateRef: MutableRefObject<WsMessagesRuntimeState>
  setMarkdownContent: Dispatch<SetStateAction<string>>
  setLogLines: Dispatch<SetStateAction<LogLine[]>>
  setErrorMessage: Dispatch<SetStateAction<string>>
  setIsProcessing: Dispatch<SetStateAction<boolean>>
  setHasResults: Dispatch<SetStateAction<boolean>>
  setCurrentMode: Dispatch<SetStateAction<string>>
  setCrawlFilesTracked: Dispatch<SetStateAction<CrawlFile[]>>
  setSelectedFileTracked: Dispatch<SetStateAction<string | null>>
  setCrawlProgress: Dispatch<SetStateAction<CrawlProgress | null>>
  setStdoutLines: Dispatch<SetStateAction<string[]>>
  setStdoutJsonTracked: Dispatch<SetStateAction<unknown[]>>
  setCommandMode: Dispatch<SetStateAction<string | null>>
  setScreenshotFiles: Dispatch<SetStateAction<ScreenshotFile[]>>
  setCurrentJobIdTracked: (jobId: string | null) => void
  setLifecycleEntries: Dispatch<SetStateAction<WsLifecycleEntry[]>>
  setCancelResponse: Dispatch<SetStateAction<CancelResponseState | null>>
  setCurrentOutputDirTracked: Dispatch<SetStateAction<string | null>>
  setVirtualFileContentByPathTracked: Dispatch<SetStateAction<Record<string, string>>>
  setWorkspaceMode: (mode: string | null) => void
  setWorkspacePrompt: (prompt: string | null) => void
  setWorkspacePromptVersion: (version: number) => void
  bumpWorkspacePromptVersion: () => void
  setWorkspaceResumeSessionId: (sessionId: string | null) => void
  setWorkspaceResumeVersion: (version: number) => void
  bumpWorkspaceResumeVersion: () => void
  setWorkspaceContext: (context: WorkspaceContextState | null) => void
  setPulseAgent: (agent: PulseAgent) => void
  setPulseModel: (model: PulseModel) => void
  setPulsePermissionLevel: (level: PulsePermissionLevel) => void
  setAcpConfigOptions: (options: AcpConfigOption[]) => void
}

export function useWsProviderActions(input: UseWsProviderActionsInput): WsMessagesActions {
  const {
    send,
    currentModeRef,
    currentInputRef,
    virtualFileContentByPathRef,
    runtimeStateRef,
    setMarkdownContent,
    setLogLines,
    setErrorMessage,
    setIsProcessing,
    setHasResults,
    setCurrentMode,
    setCrawlFilesTracked,
    setSelectedFileTracked,
    setCrawlProgress,
    setStdoutLines,
    setStdoutJsonTracked,
    setCommandMode,
    setScreenshotFiles,
    setCurrentJobIdTracked,
    setLifecycleEntries,
    setCancelResponse,
    setCurrentOutputDirTracked,
    setVirtualFileContentByPathTracked,
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
  } = input

  const resetExecutionRuntime = useCallback(
    ({ hasResults, isProcessing }: { hasResults: boolean; isProcessing: boolean }) => {
      applyResetExecutionRuntime(
        {
          setMarkdownContent,
          setLogLines,
          setErrorMessage,
          setHasResults,
          setIsProcessing,
          setCrawlFiles: setCrawlFilesTracked,
          setSelectedFile: setSelectedFileTracked,
          setVirtualFileContentByPath: setVirtualFileContentByPathTracked,
          setCurrentOutputDir: setCurrentOutputDirTracked,
          setCrawlProgress,
          setStdoutLines,
          setStdoutJson: setStdoutJsonTracked,
          setCommandMode,
          setScreenshotFiles,
          setCurrentJobId: setCurrentJobIdTracked,
          setLifecycleEntries,
          setCancelResponse,
          runtimeStateRef,
          makeInitialRuntimeState,
        },
        { hasResults, isProcessing },
      )
    },
    [
      runtimeStateRef,
      setCancelResponse,
      setCommandMode,
      setCrawlFilesTracked,
      setCrawlProgress,
      setCurrentJobIdTracked,
      setCurrentOutputDirTracked,
      setErrorMessage,
      setHasResults,
      setIsProcessing,
      setLifecycleEntries,
      setLogLines,
      setMarkdownContent,
      setScreenshotFiles,
      setSelectedFileTracked,
      setStdoutJsonTracked,
      setStdoutLines,
      setVirtualFileContentByPathTracked,
    ],
  )

  const resetWorkspaceRuntime = useCallback(
    (mode: string | null) => {
      applyResetWorkspaceRuntime(
        {
          setWorkspaceMode,
          setWorkspacePrompt,
          setWorkspacePromptVersion,
          setWorkspaceContext,
        },
        mode,
      )
    },
    [setWorkspaceContext, setWorkspaceMode, setWorkspacePrompt, setWorkspacePromptVersion],
  )

  const selectFile = useCallback(
    (relativePath: string) => {
      setSelectedFileTracked(relativePath)
      setMarkdownContent('')
      const virtualContent = virtualFileContentByPathRef.current[relativePath]
      if (typeof virtualContent === 'string') {
        setMarkdownContent(virtualContent)
        return
      }
      send({ type: 'read_file', path: relativePath })
    },
    [send, setMarkdownContent, setSelectedFileTracked, virtualFileContentByPathRef],
  )

  const startExecution = useCallback(
    (mode: string, inputText?: string, options?: { preserveWorkspace?: boolean }) => {
      applyStartExecution(
        {
          currentModeRef,
          currentInputRef,
          setCurrentMode,
          resetExecutionRuntime,
          resetWorkspaceRuntime,
        },
        mode,
        inputText,
        options,
      )
    },
    [currentInputRef, currentModeRef, resetExecutionRuntime, resetWorkspaceRuntime, setCurrentMode],
  )

  const activateWorkspace = useCallback(
    (mode: string) => {
      applyActivateWorkspace(
        {
          currentModeRef,
          currentInputRef,
          setCurrentMode,
          resetExecutionRuntime,
          resetWorkspaceRuntime,
        },
        mode,
      )
    },
    [currentInputRef, currentModeRef, resetExecutionRuntime, resetWorkspaceRuntime, setCurrentMode],
  )

  const submitWorkspacePrompt = useCallback(
    (prompt: string) => {
      applySubmitWorkspacePrompt(
        {
          setWorkspaceMode,
          setHasResults,
          setWorkspaceResumeSessionId,
          setWorkspaceResumeVersion,
          setWorkspacePrompt,
          bumpWorkspacePromptVersion,
        },
        prompt,
      )
    },
    [
      bumpWorkspacePromptVersion,
      setHasResults,
      setWorkspaceMode,
      setWorkspacePrompt,
      setWorkspaceResumeSessionId,
      setWorkspaceResumeVersion,
    ],
  )

  const resumeWorkspaceSession = useCallback(
    (sessionId: string) => {
      applyResumeWorkspaceSession(
        {
          setWorkspaceMode,
          setHasResults,
          setWorkspacePrompt,
          setWorkspacePromptVersion,
          setWorkspaceResumeSessionId,
          bumpWorkspaceResumeVersion,
        },
        sessionId,
      )
    },
    [
      bumpWorkspaceResumeVersion,
      setHasResults,
      setWorkspaceMode,
      setWorkspacePrompt,
      setWorkspacePromptVersion,
      setWorkspaceResumeSessionId,
    ],
  )

  const clearWorkspaceResumeSession = useCallback(() => {
    applyClearWorkspaceResumeSession({
      setWorkspaceResumeSessionId,
      setWorkspaceResumeVersion,
    })
  }, [setWorkspaceResumeSessionId, setWorkspaceResumeVersion])

  const deactivateWorkspace = useCallback(() => {
    applyDeactivateWorkspace({
      currentModeRef,
      currentInputRef,
      setCurrentMode,
      removeStoredWorkspaceMode: () => safeRemoveItem(LS_WORKSPACE_MODE),
      setWorkspaceMode,
      setWorkspacePrompt,
      setWorkspacePromptVersion,
      setWorkspaceResumeSessionId,
      setWorkspaceResumeVersion,
      setWorkspaceContext,
    })
  }, [
    currentInputRef,
    currentModeRef,
    setCurrentMode,
    setWorkspaceContext,
    setWorkspaceMode,
    setWorkspacePrompt,
    setWorkspacePromptVersion,
    setWorkspaceResumeSessionId,
    setWorkspaceResumeVersion,
  ])

  const updateWorkspaceContext = useCallback(
    (context: WorkspaceContextState | null) => {
      setWorkspaceContext(context)
    },
    [setWorkspaceContext],
  )

  return useMemo(
    () => ({
      selectFile,
      setPulseAgent,
      setPulseModel,
      setPulsePermissionLevel,
      setAcpConfigOptions,
      activateWorkspace,
      submitWorkspacePrompt,
      resumeWorkspaceSession,
      clearWorkspaceResumeSession,
      deactivateWorkspace,
      updateWorkspaceContext,
      startExecution,
    }),
    [
      activateWorkspace,
      clearWorkspaceResumeSession,
      deactivateWorkspace,
      resumeWorkspaceSession,
      selectFile,
      setAcpConfigOptions,
      setPulseAgent,
      setPulseModel,
      setPulsePermissionLevel,
      startExecution,
      submitWorkspacePrompt,
      updateWorkspaceContext,
    ],
  )
}
