import { useCallback, useMemo } from 'react'
import type { PromptInputFile, PromptInputMessage } from '@/components/ai-elements/prompt-input'
import type { FileEntry } from '@/components/workspace/file-tree'
import type { AxonMessage } from '@/hooks/use-axon-session'
import type { SessionSummary } from '@/hooks/use-recent-sessions'
import type { PulseAgent, PulseModel, PulsePermissionLevel } from '@/lib/pulse/types'
import type { ToolPreset } from '@/lib/shell/tool-preferences'
import { buildAgentHandoffContext, createClientId } from './axon-shell-state-helpers'
import type { RailMode } from './axon-ui-config'

type WorkspaceSummary = {
  fileEntries: FileEntry[]
  fileLoading: boolean
  selectedFilePath: string | null
  setSelectedFilePath: (path: string | null) => void
}

type McpSummary = {
  enabledMcpServers: string[]
  mcpServers: string[]
  mcpStatusByServer: Record<string, 'online' | 'offline' | 'unknown'>
  toggleMcpServer: (serverName: string) => void
  setEnabledMcpServers: (servers: string[]) => void
}

type Params = {
  bumpSessionInfoGen: () => void
  activeAssistantSessionId: string | null
  activeSessionId: string | null
  activeSessionRepo: string
  assistantSessions: SessionSummary[]
  composerFiles: PromptInputFile[]
  connected: boolean
  effectiveEnabledMcpTools: string[]
  isStreaming: boolean
  liveMessages: AxonMessage[]
  mcp: McpSummary
  mcpToolsByServer: Record<string, string[]>
  modelOptions: Array<{ value: string; label: string }>
  permissionOptions: Array<{ value: string; label: string }>
  pulseAgent: PulseAgent | null
  pulseModel: PulseModel
  railMode: RailMode
  railQuery: string
  sessions: SessionSummary[]
  sessionMode: string
  setActiveAssistantSessionId: (id: string | null) => void
  setActiveFile: (path: string) => void
  setActiveSessionId: (id: string | null) => void
  setComposerFiles: (files: PromptInputFile[]) => void
  setEnabledMcpTools: React.Dispatch<React.SetStateAction<string[] | null>>
  setLiveMessages: React.Dispatch<React.SetStateAction<AxonMessage[]>>
  setPendingHandoffContext: (context: string | null) => void
  setPulseAgent: (agent: PulseAgent) => void
  setPulseModel: (model: string) => void
  setPulsePermissionLevel: (value: PulsePermissionLevel) => void
  setRailModeTracked: (mode: RailMode) => void
  setRailQuery: (value: string) => void
  setSessionKey: React.Dispatch<React.SetStateAction<number>>
  setSessionMode: (value: string) => void
  setToolPresets: React.Dispatch<React.SetStateAction<ToolPreset[]>>
  submitPrompt: (text: string) => void
  toolPresets: ToolPreset[]
  workspace: WorkspaceSummary
  persistChatOpen: (open: boolean) => void
  persistRightPane: (
    pane: 'editor' | 'terminal' | 'logs' | 'mcp' | 'settings' | 'cortex' | null,
  ) => void
  setMobilePaneTracked: (
    pane: 'sidebar' | 'chat' | 'editor' | 'terminal' | 'logs' | 'mcp' | 'settings' | 'cortex',
  ) => void
}

export function useAxonShellActions(params: Params) {
  const {
    bumpSessionInfoGen,
    activeAssistantSessionId,
    activeSessionId,
    activeSessionRepo,
    assistantSessions,
    composerFiles,
    connected,
    effectiveEnabledMcpTools,
    isStreaming,
    liveMessages,
    mcp,
    mcpToolsByServer,
    modelOptions,
    permissionOptions,
    pulseAgent,
    pulseModel,
    railMode,
    railQuery,
    sessions,
    sessionMode,
    setActiveAssistantSessionId,
    setActiveFile,
    setActiveSessionId,
    setComposerFiles,
    setEnabledMcpTools,
    setLiveMessages,
    setPendingHandoffContext,
    setPulseAgent,
    setPulseModel,
    setPulsePermissionLevel,
    setRailModeTracked,
    setRailQuery,
    setSessionKey,
    setSessionMode,
    setToolPresets,
    submitPrompt,
    toolPresets,
    workspace,
    persistChatOpen,
    persistRightPane,
    setMobilePaneTracked,
  } = params

  const openFile = useCallback(
    (path: string) => {
      setActiveFile(path)
      workspace.setSelectedFilePath(path)
      persistRightPane('editor')
    },
    [persistRightPane, setActiveFile, workspace],
  )

  const handleSidebarFileSelect = useCallback(
    (entry: FileEntry) => {
      workspace.setSelectedFilePath(entry.path)
      if (entry.type === 'file') {
        openFile(entry.path)
      }
    },
    [openFile, workspace],
  )

  const handlePromptSubmit = useCallback(
    (message: PromptInputMessage) => {
      const text =
        message.text ||
        (message.files.length > 0
          ? `Attached ${message.files.length} file${message.files.length === 1 ? '' : 's'}.`
          : '')
      if (text) submitPrompt(text)
    },
    [submitPrompt],
  )

  const handleSelectSession = useCallback(
    (sessionId: string) => {
      bumpSessionInfoGen()
      setLiveMessages([])
      setActiveSessionId(sessionId)
      setActiveAssistantSessionId(null)
      setSessionKey((k) => k + 1)

      const session = sessions.find((s) => s.id === sessionId)
      if (session?.agent && session.agent !== (pulseAgent ?? 'claude')) {
        setPulseAgent(session.agent as PulseAgent)
        setPulseModel('default')
      }
    },
    [
      bumpSessionInfoGen,
      pulseAgent,
      sessions,
      setActiveAssistantSessionId,
      setActiveSessionId,
      setLiveMessages,
      setPulseAgent,
      setPulseModel,
      setSessionKey,
    ],
  )

  const handleSelectAssistantSession = useCallback(
    (sessionId: string) => {
      bumpSessionInfoGen()
      setLiveMessages([])
      setActiveAssistantSessionId(sessionId)
      setActiveSessionId(null)
      setSessionKey((k) => k + 1)
    },
    [
      bumpSessionInfoGen,
      setActiveAssistantSessionId,
      setActiveSessionId,
      setLiveMessages,
      setSessionKey,
    ],
  )

  const handleNewSession = useCallback(() => {
    bumpSessionInfoGen()
    setActiveSessionId(null)
    setActiveAssistantSessionId(null)
    setLiveMessages([])
    setPendingHandoffContext(null)
    setSessionKey((k) => k + 1)
  }, [
    bumpSessionInfoGen,
    setActiveAssistantSessionId,
    setActiveSessionId,
    setLiveMessages,
    setPendingHandoffContext,
    setSessionKey,
  ])

  const handleDesktopNewSession = useCallback(() => {
    handleNewSession()
    persistChatOpen(true)
  }, [handleNewSession, persistChatOpen])

  const handleMobileNewSession = useCallback(() => {
    handleNewSession()
    setMobilePaneTracked('chat')
  }, [handleNewSession, setMobilePaneTracked])

  const handleMobileSelectSession = useCallback(
    (sessionId: string) => {
      handleSelectSession(sessionId)
      setMobilePaneTracked('chat')
    },
    [handleSelectSession, setMobilePaneTracked],
  )

  const handleMobileSelectAssistantSession = useCallback(
    (sessionId: string) => {
      handleSelectAssistantSession(sessionId)
      setMobilePaneTracked('chat')
    },
    [handleSelectAssistantSession, setMobilePaneTracked],
  )

  const handleMobileFileSelect = useCallback(
    (entry: FileEntry) => {
      workspace.setSelectedFilePath(entry.path)
      if (entry.type === 'file') {
        openFile(entry.path)
        setMobilePaneTracked('editor')
      }
    },
    [openFile, setMobilePaneTracked, workspace],
  )

  const handleMobileOpenFile = useCallback(
    (path: string) => {
      openFile(path)
      setMobilePaneTracked('editor')
    },
    [openFile, setMobilePaneTracked],
  )

  const handleEditMessage = useCallback(
    (messageId: string, content: string) => {
      setLiveMessages((prev) => {
        const idx = prev.findIndex((m) => m.id === messageId)
        return idx >= 0 ? prev.slice(0, idx) : prev
      })
      submitPrompt(content)
    },
    [setLiveMessages, submitPrompt],
  )

  const handleRetryMessage = useCallback(
    (messageId: string) => {
      const idx = liveMessages.findIndex((m) => m.id === messageId)
      if (idx <= 0) return
      const userMsg = liveMessages
        .slice(0, idx)
        .reverse()
        .find((m) => m.role === 'user')
      if (!userMsg) return
      setLiveMessages((prev) => {
        const userIdx = prev.findIndex((m) => m.id === userMsg.id)
        return userIdx >= 0 ? prev.slice(0, userIdx) : prev
      })
      submitPrompt(userMsg.content)
    },
    [liveMessages, setLiveMessages, submitPrompt],
  )

  const handleEnableServerTools = useCallback(
    (serverName: string) => {
      const serverTools = mcpToolsByServer[serverName] ?? []
      if (serverTools.length === 0) return
      setEnabledMcpTools((current) => {
        const base = current ?? Object.values(mcpToolsByServer).flat()
        return Array.from(new Set([...base, ...serverTools]))
      })
    },
    [mcpToolsByServer, setEnabledMcpTools],
  )

  const handleDisableServerTools = useCallback(
    (serverName: string) => {
      const serverTools = new Set(mcpToolsByServer[serverName] ?? [])
      if (serverTools.size === 0) return
      setEnabledMcpTools((current) => {
        const base = current ?? Object.values(mcpToolsByServer).flat()
        return base.filter((toolName) => !serverTools.has(toolName))
      })
    },
    [mcpToolsByServer, setEnabledMcpTools],
  )

  const handleSaveToolPreset = useCallback(
    (name: string) => {
      const trimmed = name.trim()
      if (!trimmed) return
      const preset: ToolPreset = {
        id: createClientId(),
        name: trimmed,
        enabledMcpServers: [...mcp.enabledMcpServers],
        enabledMcpTools: [...effectiveEnabledMcpTools],
      }
      setToolPresets((current) => {
        const withoutSameName = current.filter(
          (item) => item.name.toLowerCase() !== trimmed.toLowerCase(),
        )
        return [preset, ...withoutSameName].slice(0, 50)
      })
    },
    [effectiveEnabledMcpTools, mcp.enabledMcpServers, setToolPresets],
  )

  const handleApplyToolPreset = useCallback(
    (presetId: string) => {
      const preset = toolPresets.find((item) => item.id === presetId)
      if (!preset) return
      mcp.setEnabledMcpServers(preset.enabledMcpServers)
      setEnabledMcpTools(preset.enabledMcpTools)
    },
    [mcp, setEnabledMcpTools, toolPresets],
  )

  const handleDeleteToolPreset = useCallback(
    (presetId: string) => {
      setToolPresets((current) => current.filter((item) => item.id !== presetId))
    },
    [setToolPresets],
  )

  const sidebarProps = useMemo(
    () => ({
      sessions,
      railMode,
      onRailModeChange: setRailModeTracked,
      railQuery,
      onRailQueryChange: setRailQuery,
      activeSessionId,
      activeSessionRepo,
      assistantSessions,
      activeAssistantSessionId,
      onSelectAssistantSession: handleSelectAssistantSession,
      fileEntries: workspace.fileEntries,
      fileLoading: workspace.fileLoading,
      selectedFilePath: workspace.selectedFilePath,
      onNewSession: handleDesktopNewSession,
    }),
    [
      sessions,
      railMode,
      setRailModeTracked,
      railQuery,
      setRailQuery,
      activeSessionId,
      activeSessionRepo,
      assistantSessions,
      activeAssistantSessionId,
      handleSelectAssistantSession,
      workspace.fileEntries,
      workspace.fileLoading,
      workspace.selectedFilePath,
      handleDesktopNewSession,
    ],
  )

  const composerProps = useMemo(
    () => ({
      files: composerFiles,
      onFilesChange: setComposerFiles,
      onSubmit: handlePromptSubmit,
      modelOptions,
      permissionOptions,
      pulseModel: pulseModel ?? 'sonnet',
      pulsePermissionLevel: sessionMode,
      onModelChange: (value: string) => setPulseModel(value),
      onPermissionChange: (value: string) => {
        setSessionMode(value)
        if (value === 'plan' || value === 'accept-edits' || value === 'bypass-permissions') {
          setPulsePermissionLevel(value)
        }
      },
      toolsState: {
        mcpServers: mcp.mcpServers,
        enabledMcpServers: mcp.enabledMcpServers,
        mcpStatusByServer: mcp.mcpStatusByServer,
      },
      onToggleMcpServer: mcp.toggleMcpServer,
      mcpToolsByServer,
      enabledMcpTools: effectiveEnabledMcpTools,
      onEnableServerTools: handleEnableServerTools,
      onDisableServerTools: handleDisableServerTools,
      onToggleMcpTool: (toolName: string) => {
        setEnabledMcpTools((current) => {
          const knownTools = Object.values(mcpToolsByServer).flat()
          const base = current ?? knownTools
          return base.includes(toolName)
            ? base.filter((name) => name !== toolName)
            : [...base, toolName]
        })
      },
      toolPresets: toolPresets.map((preset) => ({ id: preset.id, name: preset.name })),
      onApplyToolPreset: handleApplyToolPreset,
      onDeleteToolPreset: handleDeleteToolPreset,
      onSaveToolPreset: handleSaveToolPreset,
      pulseAgent: (pulseAgent ?? 'claude') as PulseAgent,
      onAgentChange: (value: PulseAgent) => {
        const fromAgent = pulseAgent ?? 'claude'
        if (value !== fromAgent) {
          const handoff = buildAgentHandoffContext(liveMessages, fromAgent, value)
          setPendingHandoffContext(handoff || null)
        }
        setPulseAgent(value)
        setPulseModel('default')
      },
      isStreaming,
      connected,
    }),
    [
      composerFiles,
      setComposerFiles,
      handlePromptSubmit,
      modelOptions,
      permissionOptions,
      pulseModel,
      sessionMode,
      setPulseModel,
      setSessionMode,
      setPulsePermissionLevel,
      mcp.mcpServers,
      mcp.enabledMcpServers,
      mcp.mcpStatusByServer,
      mcp.toggleMcpServer,
      mcpToolsByServer,
      effectiveEnabledMcpTools,
      handleEnableServerTools,
      handleDisableServerTools,
      setEnabledMcpTools,
      toolPresets,
      handleApplyToolPreset,
      handleDeleteToolPreset,
      handleSaveToolPreset,
      pulseAgent,
      liveMessages,
      setPendingHandoffContext,
      setPulseAgent,
      isStreaming,
      connected,
    ],
  )

  const handleStats = useCallback((_data: unknown) => {
    // DockerStats manages its own state; this hook is reserved for future use
  }, [])

  return {
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
    handleStats,
    openFile,
    sidebarProps,
  }
}
