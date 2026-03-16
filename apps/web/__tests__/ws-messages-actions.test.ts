import { describe, expect, it, vi } from 'vitest'
import {
  applyActivateWorkspace,
  applyClearWorkspaceResumeSession,
  applyDeactivateWorkspace,
  applyResetExecutionRuntime,
  applyResetWorkspaceRuntime,
  applyResumeWorkspaceSession,
  applyStartExecution,
  applySubmitWorkspacePrompt,
} from '@/hooks/ws-messages/actions'

describe('applySubmitWorkspacePrompt', () => {
  it('activates pulse mode, clears resume state, and bumps prompt version', () => {
    const deps = {
      setWorkspaceMode: vi.fn(),
      setHasResults: vi.fn(),
      setWorkspaceResumeSessionId: vi.fn(),
      setWorkspaceResumeVersion: vi.fn(),
      setWorkspacePrompt: vi.fn(),
      bumpWorkspacePromptVersion: vi.fn(),
    }

    applySubmitWorkspacePrompt(deps, 'Summarize the latest crawl')

    expect(deps.setWorkspaceMode).toHaveBeenCalledWith('pulse')
    expect(deps.setHasResults).toHaveBeenCalledWith(true)
    expect(deps.setWorkspaceResumeSessionId).toHaveBeenCalledWith(null)
    expect(deps.setWorkspaceResumeVersion).toHaveBeenCalledWith(0)
    expect(deps.setWorkspacePrompt).toHaveBeenCalledWith('Summarize the latest crawl')
    expect(deps.bumpWorkspacePromptVersion).toHaveBeenCalledOnce()
  })
})

describe('applyResumeWorkspaceSession', () => {
  it('activates pulse mode, clears prompt state, and bumps resume version', () => {
    const deps = {
      setWorkspaceMode: vi.fn(),
      setHasResults: vi.fn(),
      setWorkspacePrompt: vi.fn(),
      setWorkspacePromptVersion: vi.fn(),
      setWorkspaceResumeSessionId: vi.fn(),
      bumpWorkspaceResumeVersion: vi.fn(),
    }

    applyResumeWorkspaceSession(deps, 'session-42')

    expect(deps.setWorkspaceMode).toHaveBeenCalledWith('pulse')
    expect(deps.setHasResults).toHaveBeenCalledWith(true)
    expect(deps.setWorkspacePrompt).toHaveBeenCalledWith(null)
    expect(deps.setWorkspacePromptVersion).toHaveBeenCalledWith(0)
    expect(deps.setWorkspaceResumeSessionId).toHaveBeenCalledWith('session-42')
    expect(deps.bumpWorkspaceResumeVersion).toHaveBeenCalledOnce()
  })
})

describe('applyStartExecution', () => {
  it('updates current refs, current mode, resets runtime, and clears workspace when not preserved', () => {
    const deps = {
      currentModeRef: { current: '' },
      currentInputRef: { current: '' },
      setCurrentMode: vi.fn(),
      resetExecutionRuntime: vi.fn(),
      resetWorkspaceRuntime: vi.fn(),
    }

    applyStartExecution(deps, 'crawl', 'https://example.com', { preserveWorkspace: false })

    expect(deps.currentModeRef.current).toBe('crawl')
    expect(deps.currentInputRef.current).toBe('https://example.com')
    expect(deps.setCurrentMode).toHaveBeenCalledWith('crawl')
    expect(deps.resetExecutionRuntime).toHaveBeenCalledWith({
      hasResults: true,
      isProcessing: true,
    })
    expect(deps.resetWorkspaceRuntime).toHaveBeenCalledWith(null)
  })

  it('keeps workspace state when preserveWorkspace is true', () => {
    const deps = {
      currentModeRef: { current: '' },
      currentInputRef: { current: '' },
      setCurrentMode: vi.fn(),
      resetExecutionRuntime: vi.fn(),
      resetWorkspaceRuntime: vi.fn(),
    }

    applyStartExecution(deps, 'extract', undefined, { preserveWorkspace: true })

    expect(deps.currentModeRef.current).toBe('extract')
    expect(deps.currentInputRef.current).toBe('')
    expect(deps.resetWorkspaceRuntime).not.toHaveBeenCalled()
  })
})

describe('applyActivateWorkspace', () => {
  it('switches mode, clears current input, and resets runtime for workspace activation', () => {
    const deps = {
      currentModeRef: { current: '' },
      currentInputRef: { current: 'stale input' },
      setCurrentMode: vi.fn(),
      resetExecutionRuntime: vi.fn(),
      resetWorkspaceRuntime: vi.fn(),
    }

    applyActivateWorkspace(deps, 'pulse')

    expect(deps.currentModeRef.current).toBe('pulse')
    expect(deps.currentInputRef.current).toBe('')
    expect(deps.setCurrentMode).toHaveBeenCalledWith('pulse')
    expect(deps.resetExecutionRuntime).toHaveBeenCalledWith({
      hasResults: false,
      isProcessing: false,
    })
    expect(deps.resetWorkspaceRuntime).toHaveBeenCalledWith('pulse')
  })
})

describe('applyClearWorkspaceResumeSession', () => {
  it('clears the resume session id and resets the resume version', () => {
    const deps = {
      setWorkspaceResumeSessionId: vi.fn(),
      setWorkspaceResumeVersion: vi.fn(),
    }

    applyClearWorkspaceResumeSession(deps)

    expect(deps.setWorkspaceResumeSessionId).toHaveBeenCalledWith(null)
    expect(deps.setWorkspaceResumeVersion).toHaveBeenCalledWith(0)
  })
})

describe('applyDeactivateWorkspace', () => {
  it('clears current refs, resets current mode, removes stored mode, and clears workspace state', () => {
    const deps = {
      currentModeRef: { current: 'crawl' },
      currentInputRef: { current: 'https://example.com' },
      setCurrentMode: vi.fn(),
      removeStoredWorkspaceMode: vi.fn(),
      setWorkspaceMode: vi.fn(),
      setWorkspacePrompt: vi.fn(),
      setWorkspacePromptVersion: vi.fn(),
      setWorkspaceResumeSessionId: vi.fn(),
      setWorkspaceResumeVersion: vi.fn(),
      setWorkspaceContext: vi.fn(),
    }

    applyDeactivateWorkspace(deps)

    expect(deps.currentModeRef.current).toBe('')
    expect(deps.currentInputRef.current).toBe('')
    expect(deps.setCurrentMode).toHaveBeenCalledWith('')
    expect(deps.removeStoredWorkspaceMode).toHaveBeenCalledOnce()
    expect(deps.setWorkspaceMode).toHaveBeenCalledWith(null)
    expect(deps.setWorkspacePrompt).toHaveBeenCalledWith(null)
    expect(deps.setWorkspacePromptVersion).toHaveBeenCalledWith(0)
    expect(deps.setWorkspaceResumeSessionId).toHaveBeenCalledWith(null)
    expect(deps.setWorkspaceResumeVersion).toHaveBeenCalledWith(0)
    expect(deps.setWorkspaceContext).toHaveBeenCalledWith(null)
  })
})

describe('applyResetExecutionRuntime', () => {
  it('clears execution state fields and replaces runtimeStateRef with a fresh snapshot', () => {
    const runtimeStateRef = {
      current: {
        currentJobId: 'job-1',
        commandMode: 'crawl',
        markdownContent: '# old',
        crawlProgress: {
          pages_crawled: 1,
          pages_discovered: 2,
          md_created: 1,
          thin_md: 0,
          phase: 'done',
        },
        screenshotFiles: [{ path: 'a.png', name: 'a.png' }],
        lifecycleEntries: [],
        stdoutJson: [{ ok: true }],
        cancelResponse: { ok: true, message: 'old' },
      },
    }
    const deps = {
      setMarkdownContent: vi.fn(),
      setLogLines: vi.fn(),
      setErrorMessage: vi.fn(),
      setHasResults: vi.fn(),
      setIsProcessing: vi.fn(),
      setCrawlFiles: vi.fn(),
      setSelectedFile: vi.fn(),
      setVirtualFileContentByPath: vi.fn(),
      setCurrentOutputDir: vi.fn(),
      setCrawlProgress: vi.fn(),
      setStdoutLines: vi.fn(),
      setStdoutJson: vi.fn(),
      setCommandMode: vi.fn(),
      setScreenshotFiles: vi.fn(),
      setCurrentJobId: vi.fn(),
      setLifecycleEntries: vi.fn(),
      setCancelResponse: vi.fn(),
      runtimeStateRef,
      makeInitialRuntimeState: () => ({
        currentJobId: null,
        commandMode: null,
        markdownContent: '',
        crawlProgress: null,
        screenshotFiles: [],
        lifecycleEntries: [],
        stdoutJson: [],
        cancelResponse: null,
      }),
    }

    applyResetExecutionRuntime(deps, { hasResults: false, isProcessing: true })

    expect(deps.setMarkdownContent).toHaveBeenCalledWith('')
    expect(deps.setLogLines).toHaveBeenCalledWith([])
    expect(deps.setErrorMessage).toHaveBeenCalledWith('')
    expect(deps.setHasResults).toHaveBeenCalledWith(false)
    expect(deps.setIsProcessing).toHaveBeenCalledWith(true)
    expect(deps.setCrawlFiles).toHaveBeenCalledWith([])
    expect(deps.setSelectedFile).toHaveBeenCalledWith(null)
    expect(deps.setVirtualFileContentByPath).toHaveBeenCalledWith({})
    expect(deps.setCurrentOutputDir).toHaveBeenCalledWith(null)
    expect(deps.setCrawlProgress).toHaveBeenCalledWith(null)
    expect(deps.setStdoutLines).toHaveBeenCalledWith([])
    expect(deps.setStdoutJson).toHaveBeenCalledWith([])
    expect(deps.setCommandMode).toHaveBeenCalledWith(null)
    expect(deps.setScreenshotFiles).toHaveBeenCalledWith([])
    expect(deps.setCurrentJobId).toHaveBeenCalledWith(null)
    expect(deps.setLifecycleEntries).toHaveBeenCalledWith([])
    expect(deps.setCancelResponse).toHaveBeenCalledWith(null)
    expect(runtimeStateRef.current).toEqual({
      currentJobId: null,
      commandMode: null,
      markdownContent: '',
      crawlProgress: null,
      screenshotFiles: [],
      lifecycleEntries: [],
      stdoutJson: [],
      cancelResponse: null,
    })
  })
})

describe('applyResetWorkspaceRuntime', () => {
  it('sets mode and clears prompt/context fields', () => {
    const deps = {
      setWorkspaceMode: vi.fn(),
      setWorkspacePrompt: vi.fn(),
      setWorkspacePromptVersion: vi.fn(),
      setWorkspaceContext: vi.fn(),
    }

    applyResetWorkspaceRuntime(deps, 'pulse')

    expect(deps.setWorkspaceMode).toHaveBeenCalledWith('pulse')
    expect(deps.setWorkspacePrompt).toHaveBeenCalledWith(null)
    expect(deps.setWorkspacePromptVersion).toHaveBeenCalledWith(0)
    expect(deps.setWorkspaceContext).toHaveBeenCalledWith(null)
  })
})
