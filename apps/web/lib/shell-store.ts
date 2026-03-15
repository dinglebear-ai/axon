'use client'

/**
 * Shell Zustand store — replaces the 62-field God Hook pattern in useAxonShellState.
 *
 * Slices are intentionally separated by update frequency:
 *   messages  — high-frequency: updated every streaming token
 *   streaming — high-frequency: toggles on every turn start/end
 *   session   — medium: changes on session switch
 *   editor    — medium: changes on file open/editor writes
 *   layout    — low: user-triggered pane/resize interactions
 *   settings  — rare: settings panel changes
 *
 * Each component subscribes to only the slice fields it needs,
 * so a streaming token update no longer re-renders layout components.
 */

import { create } from 'zustand'
import { subscribeWithSelector } from 'zustand/middleware'
import { useShallow } from 'zustand/shallow'
import type {
  AxonDensity,
  AxonMobilePane,
  RightPane,
} from '@/components/shell/axon-shell-state-helpers'
import type { RailMode } from '@/components/shell/axon-ui-config'
import type { AxonMessage } from '@/hooks/use-axon-session'
import type { WorkspaceContextState } from '@/hooks/ws-messages/types'
import {
  DEFAULT_NEURAL_CANVAS_PROFILE,
  type NeuralCanvasProfile,
} from '@/lib/pulse/neural-canvas-presets'
import type {
  AcpConfigOption,
  PulseAgent,
  PulseModel,
  PulsePermissionLevel,
} from '@/lib/pulse/types'

// ---------------------------------------------------------------------------
// Slice: messages
// ---------------------------------------------------------------------------
export type MessagesSlice = {
  liveMessages: AxonMessage[]
  liveMessagesHydrated: boolean
  setLiveMessages: (updater: AxonMessage[] | ((prev: AxonMessage[]) => AxonMessage[])) => void
  setLiveMessagesHydrated: (hydrated: boolean) => void
}

// ---------------------------------------------------------------------------
// Slice: streaming
// ---------------------------------------------------------------------------
export type StreamingSlice = {
  isStreaming: boolean
  connected: boolean
  setIsStreaming: (streaming: boolean) => void
  setConnected: (connected: boolean) => void
}

// ---------------------------------------------------------------------------
// Slice: session
// ---------------------------------------------------------------------------
export type SessionSlice = {
  activeSessionId: string | null
  activeAssistantSessionId: string | null
  sessionKey: number
  pendingHandoffContext: string | null
  sessionMode: string
  setActiveSessionId: (id: string | null) => void
  setActiveAssistantSessionId: (id: string | null) => void
  incrementSessionKey: () => void
  setPendingHandoffContext: (ctx: string | null) => void
  setSessionMode: (mode: string) => void
}

// ---------------------------------------------------------------------------
// Slice: editor
// ---------------------------------------------------------------------------
export type EditorSlice = {
  editorMarkdown: string
  activeFile: string
  setEditorMarkdown: (md: string | ((prev: string) => string)) => void
  setActiveFile: (path: string) => void
}

// ---------------------------------------------------------------------------
// Slice: layout
// ---------------------------------------------------------------------------
export type LayoutSlice = {
  railMode: RailMode
  mobilePane: AxonMobilePane
  sidebarOpen: boolean
  chatOpen: boolean
  rightPane: RightPane
  density: AxonDensity
  canvasProfile: NeuralCanvasProfile
  sidebarWidth: number
  chatFlex: number
  isDragging: boolean
  layoutRestored: boolean
  railQuery: string
  setRailMode: (mode: RailMode) => void
  setMobilePane: (pane: AxonMobilePane) => void
  setSidebarOpen: (open: boolean) => void
  setChatOpen: (open: boolean) => void
  setRightPane: (pane: RightPane) => void
  setDensity: (density: AxonDensity) => void
  setCanvasProfile: (profile: NeuralCanvasProfile) => void
  setSidebarWidth: (width: number | ((prev: number) => number)) => void
  setChatFlex: (flex: number | ((prev: number) => number)) => void
  setIsDragging: (dragging: boolean) => void
  setLayoutRestored: (restored: boolean) => void
  setRailQuery: (query: string) => void
}

// ---------------------------------------------------------------------------
// Slice: settings
// ---------------------------------------------------------------------------
export type SettingsSlice = {
  enableFs: boolean
  enableTerminal: boolean
  permissionTimeoutSecs: number | null
  adapterTimeoutSecs: number | null
  setEnableFs: (v: boolean) => void
  setEnableTerminal: (v: boolean) => void
  setPermissionTimeoutSecs: (v: number | null) => void
  setAdapterTimeoutSecs: (v: number | null) => void
}

// ---------------------------------------------------------------------------
// Slice: pulse
// ---------------------------------------------------------------------------
export type PulseSlice = {
  pulseAgent: PulseAgent
  pulseModel: PulseModel
  pulsePermissionLevel: PulsePermissionLevel
  acpConfigOptions: AcpConfigOption[]
  setPulseAgent: (agent: PulseAgent) => void
  setPulseModel: (model: PulseModel) => void
  setPulsePermissionLevel: (level: PulsePermissionLevel) => void
  setAcpConfigOptions: (options: AcpConfigOption[]) => void
}

// ---------------------------------------------------------------------------
// Slice: workspace
// ---------------------------------------------------------------------------
export type WorkspaceSlice = {
  workspaceMode: string | null
  workspacePrompt: string | null
  workspacePromptVersion: number
  workspaceResumeSessionId: string | null
  workspaceResumeVersion: number
  workspaceContext: WorkspaceContextState | null
  setWorkspaceMode: (mode: string | null) => void
  setWorkspacePrompt: (prompt: string | null) => void
  setWorkspacePromptVersion: (version: number) => void
  bumpWorkspacePromptVersion: () => void
  setWorkspaceResumeSessionId: (sessionId: string | null) => void
  setWorkspaceResumeVersion: (version: number) => void
  bumpWorkspaceResumeVersion: () => void
  setWorkspaceContext: (context: WorkspaceContextState | null) => void
}

// ---------------------------------------------------------------------------
// Combined store type
// ---------------------------------------------------------------------------
export type ShellStore = MessagesSlice &
  StreamingSlice &
  SessionSlice &
  EditorSlice &
  LayoutSlice &
  SettingsSlice &
  PulseSlice &
  WorkspaceSlice

// ---------------------------------------------------------------------------
// Store implementation
// ---------------------------------------------------------------------------
export const useShellStore = create<ShellStore>()(
  subscribeWithSelector((set) => ({
    // --- messages ---
    liveMessages: [],
    liveMessagesHydrated: false,
    setLiveMessages: (updater) =>
      set((state) => ({
        liveMessages: typeof updater === 'function' ? updater(state.liveMessages) : updater,
      })),
    setLiveMessagesHydrated: (hydrated) => set({ liveMessagesHydrated: hydrated }),

    // --- streaming ---
    isStreaming: false,
    connected: false,
    setIsStreaming: (isStreaming) => set({ isStreaming }),
    setConnected: (connected) => set({ connected }),

    // --- session ---
    activeSessionId: null,
    activeAssistantSessionId: null,
    sessionKey: 0,
    pendingHandoffContext: null,
    sessionMode: '',
    setActiveSessionId: (activeSessionId) => set({ activeSessionId }),
    setActiveAssistantSessionId: (activeAssistantSessionId) => set({ activeAssistantSessionId }),
    incrementSessionKey: () => set((state) => ({ sessionKey: state.sessionKey + 1 })),
    setPendingHandoffContext: (pendingHandoffContext) => set({ pendingHandoffContext }),
    setSessionMode: (sessionMode) => set({ sessionMode }),

    // --- editor ---
    editorMarkdown: '# New document\n',
    activeFile: '',
    setEditorMarkdown: (md) =>
      set((state) => ({
        editorMarkdown: typeof md === 'function' ? md(state.editorMarkdown) : md,
      })),
    setActiveFile: (activeFile) => set({ activeFile }),

    // --- layout ---
    railMode: 'sessions',
    mobilePane: 'chat',
    sidebarOpen: true,
    chatOpen: true,
    rightPane: 'editor',
    density: 'high',
    canvasProfile: DEFAULT_NEURAL_CANVAS_PROFILE,
    sidebarWidth: 260,
    chatFlex: 1,
    isDragging: false,
    layoutRestored: false,
    railQuery: '',
    setRailMode: (railMode) => set({ railMode }),
    setMobilePane: (mobilePane) => set({ mobilePane }),
    setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
    setChatOpen: (chatOpen) => set({ chatOpen }),
    setRightPane: (rightPane) => set({ rightPane }),
    setDensity: (density) => set({ density }),
    setCanvasProfile: (canvasProfile) => set({ canvasProfile }),
    setSidebarWidth: (width) =>
      set((state) => ({
        sidebarWidth: typeof width === 'function' ? width(state.sidebarWidth) : width,
      })),
    setChatFlex: (flex) =>
      set((state) => ({
        chatFlex: typeof flex === 'function' ? flex(state.chatFlex) : flex,
      })),
    setIsDragging: (isDragging) => set({ isDragging }),
    setLayoutRestored: (layoutRestored) => set({ layoutRestored }),
    setRailQuery: (railQuery) => set({ railQuery }),

    // --- settings ---
    enableFs: true,
    enableTerminal: true,
    permissionTimeoutSecs: null,
    adapterTimeoutSecs: null,
    setEnableFs: (enableFs) => set({ enableFs }),
    setEnableTerminal: (enableTerminal) => set({ enableTerminal }),
    setPermissionTimeoutSecs: (permissionTimeoutSecs) => set({ permissionTimeoutSecs }),
    setAdapterTimeoutSecs: (adapterTimeoutSecs) => set({ adapterTimeoutSecs }),

    // --- pulse ---
    pulseAgent: 'claude',
    pulseModel: 'sonnet',
    pulsePermissionLevel: 'accept-edits',
    acpConfigOptions: [],
    setPulseAgent: (pulseAgent) => set({ pulseAgent }),
    setPulseModel: (pulseModel) => set({ pulseModel }),
    setPulsePermissionLevel: (pulsePermissionLevel) => set({ pulsePermissionLevel }),
    setAcpConfigOptions: (acpConfigOptions) => set({ acpConfigOptions }),

    // --- workspace ---
    workspaceMode: 'pulse',
    workspacePrompt: null,
    workspacePromptVersion: 0,
    workspaceResumeSessionId: null,
    workspaceResumeVersion: 0,
    workspaceContext: null,
    setWorkspaceMode: (workspaceMode) => set({ workspaceMode }),
    setWorkspacePrompt: (workspacePrompt) => set({ workspacePrompt }),
    setWorkspacePromptVersion: (workspacePromptVersion) => set({ workspacePromptVersion }),
    bumpWorkspacePromptVersion: () =>
      set((state) => ({ workspacePromptVersion: state.workspacePromptVersion + 1 })),
    setWorkspaceResumeSessionId: (workspaceResumeSessionId) => set({ workspaceResumeSessionId }),
    setWorkspaceResumeVersion: (workspaceResumeVersion) => set({ workspaceResumeVersion }),
    bumpWorkspaceResumeVersion: () =>
      set((state) => ({ workspaceResumeVersion: state.workspaceResumeVersion + 1 })),
    setWorkspaceContext: (workspaceContext) => set({ workspaceContext }),
  })),
)

// ---------------------------------------------------------------------------
// Typed slice selectors — use these in components for surgical subscriptions
// ---------------------------------------------------------------------------

/** Subscribe to only the messages slice */
export const useMessagesSlice = () =>
  useShellStore(
    useShallow((s) => ({
      liveMessages: s.liveMessages,
      liveMessagesHydrated: s.liveMessagesHydrated,
      setLiveMessages: s.setLiveMessages,
      setLiveMessagesHydrated: s.setLiveMessagesHydrated,
    })),
  )

/** Subscribe to only the streaming slice */
export const useStreamingSlice = () =>
  useShellStore(
    useShallow((s) => ({
      isStreaming: s.isStreaming,
      connected: s.connected,
      setIsStreaming: s.setIsStreaming,
      setConnected: s.setConnected,
    })),
  )

/** Subscribe to only the session slice */
export const useSessionSlice = () =>
  useShellStore(
    useShallow((s) => ({
      activeSessionId: s.activeSessionId,
      activeAssistantSessionId: s.activeAssistantSessionId,
      sessionKey: s.sessionKey,
      pendingHandoffContext: s.pendingHandoffContext,
      sessionMode: s.sessionMode,
      setActiveSessionId: s.setActiveSessionId,
      setActiveAssistantSessionId: s.setActiveAssistantSessionId,
      incrementSessionKey: s.incrementSessionKey,
      setPendingHandoffContext: s.setPendingHandoffContext,
      setSessionMode: s.setSessionMode,
    })),
  )

/** Subscribe to only the editor slice */
export const useEditorSlice = () =>
  useShellStore(
    useShallow((s) => ({
      editorMarkdown: s.editorMarkdown,
      activeFile: s.activeFile,
      setEditorMarkdown: s.setEditorMarkdown,
      setActiveFile: s.setActiveFile,
    })),
  )

/** Subscribe to only the layout slice */
export const useLayoutSlice = () =>
  useShellStore(
    useShallow((s) => ({
      railMode: s.railMode,
      mobilePane: s.mobilePane,
      sidebarOpen: s.sidebarOpen,
      chatOpen: s.chatOpen,
      rightPane: s.rightPane,
      density: s.density,
      canvasProfile: s.canvasProfile,
      sidebarWidth: s.sidebarWidth,
      chatFlex: s.chatFlex,
      isDragging: s.isDragging,
      layoutRestored: s.layoutRestored,
      railQuery: s.railQuery,
      setRailMode: s.setRailMode,
      setMobilePane: s.setMobilePane,
      setSidebarOpen: s.setSidebarOpen,
      setChatOpen: s.setChatOpen,
      setRightPane: s.setRightPane,
      setDensity: s.setDensity,
      setCanvasProfile: s.setCanvasProfile,
      setSidebarWidth: s.setSidebarWidth,
      setChatFlex: s.setChatFlex,
      setIsDragging: s.setIsDragging,
      setLayoutRestored: s.setLayoutRestored,
      setRailQuery: s.setRailQuery,
    })),
  )

/** Subscribe to only the settings slice */
export const useSettingsSlice = () =>
  useShellStore(
    useShallow((s) => ({
      enableFs: s.enableFs,
      enableTerminal: s.enableTerminal,
      permissionTimeoutSecs: s.permissionTimeoutSecs,
      adapterTimeoutSecs: s.adapterTimeoutSecs,
      setEnableFs: s.setEnableFs,
      setEnableTerminal: s.setEnableTerminal,
      setPermissionTimeoutSecs: s.setPermissionTimeoutSecs,
      setAdapterTimeoutSecs: s.setAdapterTimeoutSecs,
    })),
  )

/** Subscribe to only the pulse slice */
export const usePulseSlice = () =>
  useShellStore(
    useShallow((s) => ({
      pulseAgent: s.pulseAgent,
      pulseModel: s.pulseModel,
      pulsePermissionLevel: s.pulsePermissionLevel,
      acpConfigOptions: s.acpConfigOptions,
      setPulseAgent: s.setPulseAgent,
      setPulseModel: s.setPulseModel,
      setPulsePermissionLevel: s.setPulsePermissionLevel,
      setAcpConfigOptions: s.setAcpConfigOptions,
    })),
  )

/** Subscribe to only the workspace slice */
export const useWorkspaceSlice = () =>
  useShellStore(
    useShallow((s) => ({
      workspaceMode: s.workspaceMode,
      workspacePrompt: s.workspacePrompt,
      workspacePromptVersion: s.workspacePromptVersion,
      workspaceResumeSessionId: s.workspaceResumeSessionId,
      workspaceResumeVersion: s.workspaceResumeVersion,
      workspaceContext: s.workspaceContext,
      setWorkspaceMode: s.setWorkspaceMode,
      setWorkspacePrompt: s.setWorkspacePrompt,
      setWorkspacePromptVersion: s.setWorkspacePromptVersion,
      bumpWorkspacePromptVersion: s.bumpWorkspacePromptVersion,
      setWorkspaceResumeSessionId: s.setWorkspaceResumeSessionId,
      setWorkspaceResumeVersion: s.setWorkspaceResumeVersion,
      bumpWorkspaceResumeVersion: s.bumpWorkspaceResumeVersion,
      setWorkspaceContext: s.setWorkspaceContext,
    })),
  )
