'use client'

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { PromptInputFile } from '@/components/ai-elements/prompt-input'
import type { NeuralCanvasHandle } from '@/components/neural-canvas'
import { useCopyFeedback } from '@/hooks/use-copy-feedback'
import { useMcpServers } from '@/hooks/use-mcp-servers'
import { useWorkspaceFiles } from '@/hooks/use-workspace-files'
import { apiFetch } from '@/lib/api-fetch'
import { getAcpModeConfigOption, getAcpModelConfigOption } from '@/lib/pulse/acp-config'
import { persistToolPreferences, TOOL_PREFERENCES_LS_KEY } from '@/lib/shell/tool-preferences'
import { useShellStore } from '@/lib/shell-store'
import { useAxonShellActions } from './axon-shell-state-actions'
import { useAxonShellConnection } from './axon-shell-state-connection'
import {
  type AxonMobilePane,
  agentDisplayName,
  buildEditorMarkdown,
  type RightPane,
  shouldReloadSessionOnTurnComplete,
} from './axon-shell-state-helpers'
import { useAxonShellLayoutControls } from './axon-shell-state-layout'
import { useAxonShellMessages } from './axon-shell-state-messages'
import { useAxonShellSession } from './axon-shell-state-session'
import { useAxonShellSettings } from './axon-shell-state-settings'
import { useToolPreferenceState } from './axon-shell-state-tools'
import { AXON_PERMISSION_OPTIONS } from './axon-ui-config'
import { mergeHistoricalMessages, shouldSyncHistoricalMessages } from './live-message-sync'

export type { AxonMobilePane, RightPane }
export { PANE_WIDTH_MIN, shouldReloadSessionOnTurnComplete } from './axon-shell-state-helpers'

export function useAxonShellState() {
  const { copiedId, copy: copyMessage } = useCopyFeedback()
  const mcp = useMcpServers()
  const workspace = useWorkspaceFiles()
  const layout = useAxonShellLayoutControls()
  const session = useAxonShellSession(layout.railMode)
  const messages = useAxonShellMessages()
  const settings = useAxonShellSettings()

  // composerFiles: local state — file attachments are rare and not a re-render bottleneck
  const [composerFiles, setComposerFiles] = useState<PromptInputFile[]>([])

  const canvasRef = useRef<NeuralCanvasHandle>(null)

  // Fields migrated to Zustand store (session / editor slices)
  const sessionKey = useShellStore((s) => s.sessionKey)
  const pendingHandoffContext = useShellStore((s) => s.pendingHandoffContext)
  const setPendingHandoffContext = useShellStore((s) => s.setPendingHandoffContext)
  const sessionMode = useShellStore((s) => s.sessionMode)
  const setSessionMode = useShellStore((s) => s.setSessionMode)
  const pulseAgent = useShellStore((s) => s.pulseAgent)
  const pulseModel = useShellStore((s) => s.pulseModel)
  const _pulsePermissionLevel = useShellStore((s) => s.pulsePermissionLevel)
  const acpConfigOptions = useShellStore((s) => s.acpConfigOptions)
  const setPulseAgent = useShellStore((s) => s.setPulseAgent)
  const setPulseModel = useShellStore((s) => s.setPulseModel)
  const setPulsePermissionLevel = useShellStore((s) => s.setPulsePermissionLevel)
  const setAcpConfigOptions = useShellStore((s) => s.setAcpConfigOptions)
  const editorMarkdown = useShellStore((s) => s.editorMarkdown)
  const setEditorMarkdown = useShellStore((s) => s.setEditorMarkdown)
  const setActiveFile = useShellStore((s) => s.setActiveFile)
  const activeFile = useShellStore((s) => s.activeFile)
  const railQuery = useShellStore((s) => s.railQuery)
  const setRailQuery = useShellStore((s) => s.setRailQuery)

  // setSessionKey compatible with React.Dispatch<SetStateAction<number>>
  const setSessionKey = useCallback((updater: number | ((prev: number) => number)) => {
    if (typeof updater === 'function') {
      const next = updater(useShellStore.getState().sessionKey)
      useShellStore.setState({ sessionKey: next })
    } else {
      useShellStore.setState({ sessionKey: updater })
    }
  }, [])

  const {
    enabledMcpTools,
    handleCommandsUpdate,
    mcpToolsByServer,
    setEnabledMcpTools,
    setToolPresets,
    toolPrefsHydrated,
    toolPresets,
  } = useToolPreferenceState({
    mcpServerCount: mcp.mcpServers.length,
    setEnabledMcpServers: mcp.setEnabledMcpServers,
  })

  const effectiveEnabledMcpTools = useMemo(() => {
    const knownTools = Object.values(mcpToolsByServer).flat()
    if (enabledMcpTools === null) return knownTools
    return enabledMcpTools
  }, [enabledMcpTools, mcpToolsByServer])

  const blockedMcpTools = useMemo(() => {
    const enabledSet = new Set(effectiveEnabledMcpTools)
    const enabledServersSet = new Set(mcp.enabledMcpServers)
    const blocked = new Set<string>()
    for (const [serverName, tools] of Object.entries(mcpToolsByServer)) {
      const serverEnabled = enabledServersSet.has(serverName)
      for (const toolName of tools) {
        if (!serverEnabled || !enabledSet.has(toolName)) {
          blocked.add(toolName)
        }
      }
    }
    return Array.from(blocked)
  }, [effectiveEnabledMcpTools, mcp.enabledMcpServers, mcpToolsByServer])

  const onTurnComplete = useCallback(() => {
    session.reloadSessions()
    if (shouldReloadSessionOnTurnComplete(session.chatSessionId)) {
      session.reloadSession()
    }
    if (layout.railMode === 'assistant') {
      session.reloadAssistantSessions()
    }
  }, [session, layout.railMode])

  const onEditorUpdate = useCallback(
    (content: string, operation: 'replace' | 'append') => {
      setEditorMarkdown((prev) => (operation === 'append' ? `${prev}\n${content}` : content))
      layout.persistRightPane('editor')
      layout.setMobilePaneTracked('editor')
    },
    [layout, setEditorMarkdown],
  )

  const onHandoffConsumed = useCallback(
    () => setPendingHandoffContext(null),
    [setPendingHandoffContext],
  )

  const { submitPrompt, isStreaming, connected, handleStats, bumpSessionInfoGen, isStreamingRef } =
    useAxonShellConnection({
      chatSessionId: session.chatSessionId,
      pulseAgent: pulseAgent ?? 'claude',
      pulseModel,
      sessionMode,
      enabledMcpServers: mcp.enabledMcpServers,
      blockedMcpTools,
      assistantMode: layout.railMode === 'assistant',
      handoffContext: pendingHandoffContext,
      onSessionIdChange: session.onSessionIdChange,
      onMessagesChange: messages.onMessagesChange,
      onAcpConfigOptionsUpdate: setAcpConfigOptions,
      onCommandsUpdate: handleCommandsUpdate,
      onHandoffConsumed,
      onTurnComplete,
      onEditorUpdate,
      canvasRef,
      enableFs: settings.enableFs,
      enableTerminal: settings.enableTerminal,
      permissionTimeoutSecs: settings.permissionTimeoutSecs,
      adapterTimeoutSecs: settings.adapterTimeoutSecs,
      railMode: layout.railMode,
    })

  const lastSyncedSessionIdRef = useRef<string | null>(null)

  useEffect(() => {
    if (!toolPrefsHydrated) return
    const payload = {
      enabledMcpServers: mcp.enabledMcpServers,
      enabledMcpTools: effectiveEnabledMcpTools,
      presets: toolPresets,
    }
    try {
      window.localStorage.setItem(TOOL_PREFERENCES_LS_KEY, JSON.stringify(payload))
    } catch {
      // Ignore localStorage write failures.
    }
    const timer = setTimeout(() => {
      void persistToolPreferences(payload)
    }, 350)
    return () => clearTimeout(timer)
  }, [effectiveEnabledMcpTools, mcp.enabledMcpServers, toolPrefsHydrated, toolPresets])

  useEffect(() => {
    if (!messages.liveMessagesHydrated) return
    const sessionChanged = lastSyncedSessionIdRef.current !== session.chatSessionId
    if (
      sessionChanged &&
      !isStreamingRef.current &&
      !session.sessionLoading &&
      !session.sessionError
    ) {
      messages.setLiveMessages(session.historicalMessages)
      lastSyncedSessionIdRef.current = session.chatSessionId
      return
    }
    const shouldSync = shouldSyncHistoricalMessages({
      isStreaming: isStreamingRef.current,
      sessionLoading: session.sessionLoading,
      sessionError: session.sessionError,
      sessionChanged,
      historicalCount: session.historicalMessages.length,
      liveCount: messages.liveMessages.length,
    })
    if (!shouldSync) return
    messages.setLiveMessages((prev) => mergeHistoricalMessages(session.historicalMessages, prev))
    lastSyncedSessionIdRef.current = session.chatSessionId
  }, [
    session.chatSessionId,
    session.historicalMessages,
    messages.liveMessages.length,
    messages.liveMessagesHydrated,
    session.sessionLoading,
    session.sessionError,
    messages.setLiveMessages,
  ])

  const activeSession = useMemo(() => {
    if (layout.railMode === 'assistant') {
      return (
        session.assistantSessions.find((s) => s.id === session.activeAssistantSessionId) ?? null
      )
    }
    return session.rawSessions.find((s) => s.id === session.activeSessionId) ?? null
  }, [
    layout.railMode,
    session.assistantSessions,
    session.activeAssistantSessionId,
    session.rawSessions,
    session.activeSessionId,
  ])

  const modelOptions = useMemo(() => {
    const modelOption = getAcpModelConfigOption(acpConfigOptions)
    if (!modelOption?.options?.length) return []
    return modelOption.options.map((option) => ({ value: option.value, label: option.name }))
  }, [acpConfigOptions])

  const permissionOptions = useMemo(() => {
    const modeOption = getAcpModeConfigOption(acpConfigOptions)
    if (!modeOption?.options?.length) {
      return AXON_PERMISSION_OPTIONS.map((option) => ({ value: option.value, label: option.label }))
    }
    return modeOption.options.map((option) => ({ value: option.value, label: option.name }))
  }, [acpConfigOptions])

  useEffect(() => {
    if (permissionOptions.length === 0) return
    if (!permissionOptions.some((opt) => opt.value === sessionMode)) {
      setSessionMode(permissionOptions[0]?.value ?? '')
    }
  }, [permissionOptions, sessionMode, setSessionMode])

  // biome-ignore lint/correctness/useExhaustiveDependencies: railMode is intentional trigger
  useEffect(() => {
    setRailQuery('')
  }, [layout.railMode])

  useEffect(() => {
    if (!activeFile) return
    let cancelled = false
    apiFetch(`/api/workspace?action=read&path=${encodeURIComponent(activeFile)}`)
      .then(async (res) => {
        const data = (await res.json()) as { type?: string; content?: string }
        if (cancelled) return
        if (data.type === 'text' && typeof data.content === 'string') {
          if (activeFile.endsWith('.md') || activeFile.endsWith('.mdx')) {
            setEditorMarkdown(data.content)
          } else {
            const language = activeFile.split('.').at(-1) ?? 'text'
            setEditorMarkdown(`# ${activeFile}\n\n\`\`\`${language}\n${data.content}\n\`\`\`\n`)
          }
        } else {
          setEditorMarkdown(buildEditorMarkdown(activeFile))
        }
      })
      .catch(() => {
        if (!cancelled) setEditorMarkdown(buildEditorMarkdown(activeFile))
      })
    return () => {
      cancelled = true
    }
  }, [activeFile, setEditorMarkdown])

  const {
    composerProps,
    handleEditMessage,
    handleMobileFileSelect,
    handleMobileNewSession,
    handleMobileOpenFile,
    handleMobileSelectAssistantSession,
    handleMobileSelectSession,
    handleRetryMessage,
    handleSelectSession,
    handleSidebarFileSelect,
    openFile,
    sidebarProps,
  } = useAxonShellActions({
    bumpSessionInfoGen,
    activeAssistantSessionId: session.activeAssistantSessionId,
    activeSessionId: session.activeSessionId,
    activeSessionRepo: activeSession?.project ?? '',
    assistantSessions: session.assistantSessions,
    composerFiles,
    connected,
    effectiveEnabledMcpTools,
    isStreaming,
    liveMessages: messages.liveMessages,
    mcp: {
      enabledMcpServers: mcp.enabledMcpServers,
      mcpServers: mcp.mcpServers,
      mcpStatusByServer: mcp.mcpStatusByServer,
      setEnabledMcpServers: mcp.setEnabledMcpServers,
      toggleMcpServer: mcp.toggleMcpServer,
    },
    mcpToolsByServer,
    modelOptions,
    permissionOptions,
    pulseAgent,
    pulseModel,
    railMode: layout.railMode,
    railQuery,
    sessions: session.rawSessions,
    sessionMode,
    setActiveAssistantSessionId: session.setActiveAssistantSessionId,
    setActiveFile,
    setActiveSessionId: session.setActiveSessionId,
    setComposerFiles,
    setEnabledMcpTools,
    setLiveMessages: messages.setLiveMessages,
    setPendingHandoffContext,
    setPulseAgent,
    setPulseModel,
    setPulsePermissionLevel,
    setRailModeTracked: layout.setRailModeTracked,
    setRailQuery,
    setSessionKey: setSessionKey as React.Dispatch<React.SetStateAction<number>>,
    setSessionMode,
    setToolPresets,
    submitPrompt,
    toolPresets,
    persistChatOpen: layout.persistChatOpen,
    persistRightPane: layout.persistRightPane,
    setMobilePaneTracked: layout.setMobilePaneTracked,
    workspace: {
      fileEntries: workspace.fileEntries,
      fileLoading: workspace.fileLoading,
      selectedFilePath: workspace.selectedFilePath,
      setSelectedFilePath: workspace.setSelectedFilePath,
    },
  })

  const displayMessages =
    session.chatSessionId !== null && messages.liveMessages.length === 0
      ? session.historicalMessages
      : messages.liveMessages

  useEffect(() => {
    if (!messages.liveMessagesHydrated) return
    messages.persistMessages(connected, session.chatSessionId, messages.liveMessages)
    return () => {
      // Flush any pending debounced write immediately on unmount so messages
      // are never lost when the component tears down mid-stream.
      messages.persistMessages(connected, session.chatSessionId, messages.liveMessages, true)
    }
  }, [session.chatSessionId, connected, messages])

  const chatTitle = activeSession?.preview?.slice(0, 60) ?? activeSession?.project ?? 'New chat'
  const agentLabel = agentDisplayName(pulseAgent ?? 'claude')

  const { layoutState, layoutActions } = layout
  const { settingsState } = settings

  const conversationState = useMemo(
    () => ({
      agentLabel,
      chatTitle,
      connected,
      copiedId,
      copyMessage,
      displayMessages,
      handleEditMessage,
      handleMobileOpenFile,
      handleRetryMessage,
      handleStats,
      isStreaming,
      liveMessages: messages.liveMessages,
      openFile,
      reloadSession: session.reloadSession,
      sessionError: session.sessionError,
      sessionKey,
      sessionLoading: session.sessionLoading,
    }),
    [
      agentLabel,
      chatTitle,
      connected,
      copiedId,
      copyMessage,
      displayMessages,
      handleEditMessage,
      handleMobileOpenFile,
      handleRetryMessage,
      handleStats,
      isStreaming,
      messages.liveMessages,
      openFile,
      session.reloadSession,
      session.sessionError,
      sessionKey,
      session.sessionLoading,
    ],
  )

  const composerState = useMemo(
    () => ({
      composerProps,
    }),
    [composerProps],
  )

  const sidebarState = useMemo(
    () => ({
      handleMobileFileSelect,
      handleMobileNewSession,
      handleMobileSelectAssistantSession,
      handleMobileSelectSession,
      handleSelectSession,
      handleSidebarFileSelect,
      sidebarProps,
    }),
    [
      handleMobileFileSelect,
      handleMobileNewSession,
      handleMobileSelectAssistantSession,
      handleMobileSelectSession,
      handleSelectSession,
      handleSidebarFileSelect,
      sidebarProps,
    ],
  )

  const editorState = useMemo(
    () => ({
      editorMarkdown,
      onEditorUpdate,
      setEditorMarkdown,
    }),
    [editorMarkdown, onEditorUpdate, setEditorMarkdown],
  )

  return useMemo(
    () => ({
      canvasRef,
      layoutState,
      layoutActions,
      settings: settingsState,
      conversation: conversationState,
      composer: composerState,
      sidebar: sidebarState,
      editor: editorState,
    }),
    [
      layoutState,
      layoutActions,
      settingsState,
      conversationState,
      composerState,
      sidebarState,
      editorState,
    ],
  )
}

export type AxonShellState = ReturnType<typeof useAxonShellState>
export type AxonShellLayoutState = AxonShellState['layoutState']
export type AxonShellLayoutActions = AxonShellState['layoutActions']
export type AxonShellConversationState = AxonShellState['conversation']
export type AxonShellComposerState = AxonShellState['composer']
export type AxonShellSidebarState = AxonShellState['sidebar']
export type AxonShellEditorState = AxonShellState['editor']
export type AxonShellSettingsState = AxonShellState['settings']
