type SubmitWorkspacePromptDeps = {
  setWorkspaceMode: (mode: string | null) => void
  setHasResults: (hasResults: boolean) => void
  setWorkspaceResumeSessionId: (sessionId: string | null) => void
  setWorkspaceResumeVersion: (version: number) => void
  setWorkspacePrompt: (prompt: string | null) => void
  bumpWorkspacePromptVersion: () => void
}

export function applySubmitWorkspacePrompt(deps: SubmitWorkspacePromptDeps, prompt: string): void {
  deps.setWorkspaceMode('pulse')
  deps.setHasResults(true)
  deps.setWorkspaceResumeSessionId(null)
  deps.setWorkspaceResumeVersion(0)
  deps.setWorkspacePrompt(prompt)
  deps.bumpWorkspacePromptVersion()
}

type ResumeWorkspaceSessionDeps = {
  setWorkspaceMode: (mode: string | null) => void
  setHasResults: (hasResults: boolean) => void
  setWorkspacePrompt: (prompt: string | null) => void
  setWorkspacePromptVersion: (version: number) => void
  setWorkspaceResumeSessionId: (sessionId: string | null) => void
  bumpWorkspaceResumeVersion: () => void
}

export function applyResumeWorkspaceSession(
  deps: ResumeWorkspaceSessionDeps,
  sessionId: string,
): void {
  deps.setWorkspaceMode('pulse')
  deps.setHasResults(true)
  deps.setWorkspacePrompt(null)
  deps.setWorkspacePromptVersion(0)
  deps.setWorkspaceResumeSessionId(sessionId)
  deps.bumpWorkspaceResumeVersion()
}

type StartExecutionDeps = {
  currentModeRef: { current: string }
  currentInputRef: { current: string }
  setCurrentMode: (mode: string) => void
  resetExecutionRuntime: (state: { hasResults: boolean; isProcessing: boolean }) => void
  resetWorkspaceRuntime: (mode: string | null) => void
}

export function applyStartExecution(
  deps: StartExecutionDeps,
  mode: string,
  input?: string,
  options?: { preserveWorkspace?: boolean },
): void {
  const preserveWorkspace = options?.preserveWorkspace === true
  deps.currentModeRef.current = mode
  deps.currentInputRef.current = input ?? ''
  deps.setCurrentMode(mode)
  deps.resetExecutionRuntime({ hasResults: true, isProcessing: true })
  if (!preserveWorkspace) {
    deps.resetWorkspaceRuntime(null)
  }
}

type ActivateWorkspaceDeps = {
  currentModeRef: { current: string }
  currentInputRef: { current: string }
  setCurrentMode: (mode: string) => void
  resetExecutionRuntime: (state: { hasResults: boolean; isProcessing: boolean }) => void
  resetWorkspaceRuntime: (mode: string | null) => void
}

export function applyActivateWorkspace(deps: ActivateWorkspaceDeps, mode: string): void {
  deps.currentModeRef.current = mode
  deps.currentInputRef.current = ''
  deps.setCurrentMode(mode)
  deps.resetExecutionRuntime({ hasResults: false, isProcessing: false })
  deps.resetWorkspaceRuntime(mode)
}

type ClearWorkspaceResumeSessionDeps = {
  setWorkspaceResumeSessionId: (sessionId: string | null) => void
  setWorkspaceResumeVersion: (version: number) => void
}

export function applyClearWorkspaceResumeSession(deps: ClearWorkspaceResumeSessionDeps): void {
  deps.setWorkspaceResumeSessionId(null)
  deps.setWorkspaceResumeVersion(0)
}

type DeactivateWorkspaceDeps = {
  currentModeRef: { current: string }
  currentInputRef: { current: string }
  setCurrentMode: (mode: string) => void
  removeStoredWorkspaceMode: () => void
  setWorkspaceMode: (mode: string | null) => void
  setWorkspacePrompt: (prompt: string | null) => void
  setWorkspacePromptVersion: (version: number) => void
  setWorkspaceResumeSessionId: (sessionId: string | null) => void
  setWorkspaceResumeVersion: (version: number) => void
  setWorkspaceContext: (context: null) => void
}

export function applyDeactivateWorkspace(deps: DeactivateWorkspaceDeps): void {
  deps.currentModeRef.current = ''
  deps.currentInputRef.current = ''
  deps.setCurrentMode('')
  deps.setWorkspaceMode(null)
  deps.removeStoredWorkspaceMode()
  deps.setWorkspacePrompt(null)
  deps.setWorkspacePromptVersion(0)
  deps.setWorkspaceResumeSessionId(null)
  deps.setWorkspaceResumeVersion(0)
  deps.setWorkspaceContext(null)
}

type ResetExecutionRuntimeDeps = {
  setMarkdownContent: (content: string) => void
  setLogLines: (lines: []) => void
  setErrorMessage: (message: string) => void
  setHasResults: (hasResults: boolean) => void
  setIsProcessing: (isProcessing: boolean) => void
  setCrawlFiles: (files: []) => void
  setSelectedFile: (path: null) => void
  setVirtualFileContentByPath: (files: Record<string, string>) => void
  setCurrentOutputDir: (dir: null) => void
  setCrawlProgress: (progress: null) => void
  setStdoutLines: (lines: []) => void
  setStdoutJson: (items: []) => void
  setCommandMode: (mode: null) => void
  setScreenshotFiles: (files: []) => void
  setCurrentJobId: (jobId: null) => void
  setLifecycleEntries: (entries: []) => void
  setCancelResponse: (response: null) => void
  runtimeStateRef: { current: unknown }
  makeInitialRuntimeState: () => unknown
}

export function applyResetExecutionRuntime(
  deps: ResetExecutionRuntimeDeps,
  state: { hasResults: boolean; isProcessing: boolean },
): void {
  deps.setMarkdownContent('')
  deps.setLogLines([])
  deps.setErrorMessage('')
  deps.setHasResults(state.hasResults)
  deps.setIsProcessing(state.isProcessing)
  deps.setCrawlFiles([])
  deps.setSelectedFile(null)
  deps.setVirtualFileContentByPath({})
  deps.setCurrentOutputDir(null)
  deps.setCrawlProgress(null)
  deps.setStdoutLines([])
  deps.setStdoutJson([])
  deps.setCommandMode(null)
  deps.setScreenshotFiles([])
  deps.setCurrentJobId(null)
  deps.setLifecycleEntries([])
  deps.setCancelResponse(null)
  deps.runtimeStateRef.current = deps.makeInitialRuntimeState()
}

type ResetWorkspaceRuntimeDeps = {
  setWorkspaceMode: (mode: string | null) => void
  setWorkspacePrompt: (prompt: string | null) => void
  setWorkspacePromptVersion: (version: number) => void
  setWorkspaceContext: (context: null) => void
}

export function applyResetWorkspaceRuntime(
  deps: ResetWorkspaceRuntimeDeps,
  mode: string | null,
): void {
  deps.setWorkspaceMode(mode)
  deps.setWorkspacePrompt(null)
  deps.setWorkspacePromptVersion(0)
  deps.setWorkspaceContext(null)
}
