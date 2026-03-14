import { describe, expect, it, vi } from 'vitest'
import type { MessageHandlerRefs, MessageHandlerSetters } from '@/hooks/ws-messages/handlers'
import { handleWsMessage } from '@/hooks/ws-messages/handlers'
import { MAX_STDOUT_ITEMS, makeInitialRuntimeState } from '@/hooks/ws-messages/runtime'
import type { WsServerMsg } from '@/lib/ws-protocol'

function makeRefs(overrides: Partial<MessageHandlerRefs> = {}): MessageHandlerRefs {
  return {
    currentModeRef: { current: '' },
    currentInputRef: { current: '' },
    currentJobIdRef: { current: null },
    selectedFileRef: { current: null },
    crawlFilesRef: { current: [] },
    stdoutJsonRef: { current: [] },
    currentOutputDirRef: { current: null },
    virtualFileContentByPathRef: { current: {} },
    runIdCounter: { current: 0 },
    runtimeStateRef: { current: makeInitialRuntimeState() },
    ...overrides,
  }
}

function makeSetters(overrides: Partial<MessageHandlerSetters> = {}): MessageHandlerSetters {
  return {
    setLogLines: vi.fn(),
    setMarkdownContent: vi.fn(),
    setHasResults: vi.fn(),
    setCrawlFiles: vi.fn(),
    setCurrentOutputDir: vi.fn(),
    setSelectedFile: vi.fn(),
    setCrawlProgress: vi.fn(),
    setCommandMode: vi.fn(),
    setStdoutLines: vi.fn(),
    setStdoutJson: vi.fn(),
    setVirtualFileContentByPath: vi.fn(),
    setScreenshotFiles: vi.fn(),
    setLifecycleEntries: vi.fn(),
    setCancelResponse: vi.fn(),
    setIsProcessing: vi.fn(),
    setErrorMessage: vi.fn(),
    setRecentRuns: vi.fn(),
    setWorkspaceMode: vi.fn(),
    setWorkspacePrompt: vi.fn(),
    setWorkspacePromptVersion: vi.fn(),
    setCurrentJobIdTracked: vi.fn(),
    ...overrides,
  }
}

describe('handleWsMessage dispatcher', () => {
  it('dispatches log messages to setLogLines with pushCapped', () => {
    const setLogLines = vi.fn()
    const setters = makeSetters({ setLogLines })
    handleWsMessage({ type: 'log', line: 'test line' } as WsServerMsg, makeRefs(), setters)
    expect(setLogLines).toHaveBeenCalledOnce()
    // Invoke the updater function to verify it produces the right shape
    const updater = setLogLines.mock.calls[0]![0]
    const result = updater([])
    expect(result).toHaveLength(1)
    expect(result[0]!.content).toBe('test line')
    expect(result[0]!.timestamp).toBeTypeOf('number')
  })

  it('caps log lines when array is at MAX_STDOUT_ITEMS', () => {
    const setLogLines = vi.fn()
    const setters = makeSetters({ setLogLines })
    handleWsMessage({ type: 'log', line: 'overflow' } as WsServerMsg, makeRefs(), setters)
    const updater = setLogLines.mock.calls[0]![0]
    const largeArray = Array.from({ length: MAX_STDOUT_ITEMS }, (_, i) => ({
      content: `line-${i}`,
      timestamp: i,
    }))
    const result = updater(largeArray)
    expect(result.length).toBeLessThanOrEqual(MAX_STDOUT_ITEMS)
    expect(result[result.length - 1].content).toBe('overflow')
  })

  it('dispatches file_content to setMarkdownContent and setHasResults', () => {
    const setMarkdownContent = vi.fn()
    const setHasResults = vi.fn()
    const setters = makeSetters({ setMarkdownContent, setHasResults })
    handleWsMessage(
      { type: 'file_content', content: '# Hello' } as WsServerMsg,
      makeRefs(),
      setters,
    )
    expect(setMarkdownContent).toHaveBeenCalledWith('# Hello')
    expect(setHasResults).toHaveBeenCalledWith(true)
  })

  it('dispatches crawl_files to setCrawlFiles and tracks job_id', () => {
    const setCrawlFiles = vi.fn()
    const setCurrentJobIdTracked = vi.fn()
    const setters = makeSetters({ setCrawlFiles, setCurrentJobIdTracked })
    handleWsMessage(
      {
        type: 'crawl_files',
        files: [{ url: 'https://example.com', relative_path: 'out.md', markdown_chars: 100 }],
        output_dir: '/tmp',
        job_id: 'j-1',
      } as WsServerMsg,
      makeRefs(),
      setters,
    )
    expect(setCrawlFiles).toHaveBeenCalled()
    expect(setCurrentJobIdTracked).toHaveBeenCalledWith('j-1')
  })

  it('dispatches crawl_progress to setCrawlProgress', () => {
    const setCrawlProgress = vi.fn()
    const setters = makeSetters({ setCrawlProgress })
    handleWsMessage(
      {
        type: 'crawl_progress',
        pages_crawled: 5,
        pages_discovered: 20,
        md_created: 4,
        thin_md: 1,
        phase: 'crawling',
      } as WsServerMsg,
      makeRefs(),
      setters,
    )
    expect(setCrawlProgress).toHaveBeenCalledOnce()
  })

  it('dispatches command.start — resets stdout arrays', () => {
    const setCommandMode = vi.fn()
    const setStdoutLines = vi.fn()
    const setStdoutJson = vi.fn()
    const setters = makeSetters({ setCommandMode, setStdoutLines, setStdoutJson })
    handleWsMessage(
      {
        type: 'command.start',
        data: { ctx: { exec_id: 'e-1', mode: 'crawl', input: 'https://example.com' } },
      } as WsServerMsg,
      makeRefs(),
      setters,
    )
    expect(setCommandMode).toHaveBeenCalledWith('crawl')
    expect(setStdoutLines).toHaveBeenCalledWith([])
    expect(setStdoutJson).toHaveBeenCalledWith([])
  })

  it('dispatches command.output.line — appends to stdoutLines via pushCapped', () => {
    const setStdoutLines = vi.fn()
    const setHasResults = vi.fn()
    const setters = makeSetters({ setStdoutLines, setHasResults })
    handleWsMessage(
      { type: 'command.output.line', data: { line: 'hello' } } as WsServerMsg,
      makeRefs(),
      setters,
    )
    expect(setStdoutLines).toHaveBeenCalledOnce()
    expect(setHasResults).toHaveBeenCalledWith(true)
  })

  it('dispatches command.output.json — extracts job_id and appends to stdoutJson', () => {
    const setStdoutJson = vi.fn()
    const setCurrentJobIdTracked = vi.fn()
    const setHasResults = vi.fn()
    const setters = makeSetters({ setStdoutJson, setCurrentJobIdTracked, setHasResults })
    handleWsMessage(
      {
        type: 'command.output.json',
        data: {
          ctx: { exec_id: 'e-1', mode: 'query', input: 'test' },
          data: { job_id: 'j-42', rows: 3 },
        },
      } as WsServerMsg,
      makeRefs(),
      setters,
    )
    expect(setCurrentJobIdTracked).toHaveBeenCalledWith('j-42')
    expect(setStdoutJson).toHaveBeenCalledOnce()
    expect(setHasResults).toHaveBeenCalledWith(true)
  })

  it('dispatches command.output.json — stores output_dir from data', () => {
    const setCurrentOutputDir = vi.fn()
    const setters = makeSetters({ setCurrentOutputDir })
    handleWsMessage(
      {
        type: 'command.output.json',
        data: {
          ctx: { exec_id: 'e-1', mode: 'crawl', input: 'test' },
          data: { output_dir: '/tmp/output' },
        },
      } as WsServerMsg,
      makeRefs(),
      setters,
    )
    expect(setCurrentOutputDir).toHaveBeenCalledWith('/tmp/output')
  })

  it('dispatches command.done — clears isProcessing and records recent run', () => {
    const setIsProcessing = vi.fn()
    const setRecentRuns = vi.fn()
    const refs = makeRefs()
    refs.currentModeRef.current = 'query'
    refs.currentInputRef.current = 'test input'
    const setters = makeSetters({ setIsProcessing, setRecentRuns })
    handleWsMessage(
      {
        type: 'command.done',
        data: {
          ctx: { exec_id: 'e-1', mode: 'query', input: 'test' },
          payload: { exit_code: 0, elapsed_ms: 500 },
        },
      } as WsServerMsg,
      refs,
      setters,
    )
    expect(setIsProcessing).toHaveBeenCalledWith(false)
    expect(setRecentRuns).toHaveBeenCalledOnce()
  })

  it('dispatches command.done — triggers workspace handoff for scrape/crawl/extract on exit 0', () => {
    const setWorkspaceMode = vi.fn()
    const setWorkspacePrompt = vi.fn()
    const setHasResults = vi.fn()
    const refs = makeRefs()
    refs.currentModeRef.current = 'scrape'
    refs.currentInputRef.current = 'https://example.com'
    const setters = makeSetters({ setWorkspaceMode, setWorkspacePrompt, setHasResults })
    handleWsMessage(
      {
        type: 'command.done',
        data: {
          ctx: { exec_id: 'e-1', mode: 'scrape', input: 'https://example.com' },
          payload: { exit_code: 0, elapsed_ms: 300 },
        },
      } as WsServerMsg,
      refs,
      setters,
    )
    expect(setWorkspaceMode).toHaveBeenCalledWith('pulse')
    expect(setWorkspacePrompt).toHaveBeenCalledOnce()
    expect(setHasResults).toHaveBeenCalledWith(true)
  })

  it('dispatches command.done — does NOT trigger workspace handoff on non-zero exit', () => {
    const setWorkspaceMode = vi.fn()
    const refs = makeRefs()
    refs.currentModeRef.current = 'crawl'
    const setters = makeSetters({ setWorkspaceMode })
    handleWsMessage(
      {
        type: 'command.done',
        data: {
          ctx: { exec_id: 'e-1', mode: 'crawl', input: 'test' },
          payload: { exit_code: 1, elapsed_ms: 100 },
        },
      } as WsServerMsg,
      refs,
      setters,
    )
    expect(setWorkspaceMode).not.toHaveBeenCalled()
  })

  it('dispatches command.error — sets error message and records failed run', () => {
    const setIsProcessing = vi.fn()
    const setErrorMessage = vi.fn()
    const setRecentRuns = vi.fn()
    const refs = makeRefs()
    refs.currentModeRef.current = 'embed'
    refs.currentInputRef.current = 'test.txt'
    const setters = makeSetters({ setIsProcessing, setErrorMessage, setRecentRuns })
    handleWsMessage(
      {
        type: 'command.error',
        data: {
          ctx: { exec_id: 'e-1', mode: 'embed', input: 'test.txt' },
          payload: { message: 'connection refused', elapsed_ms: 50 },
        },
      } as WsServerMsg,
      refs,
      setters,
    )
    expect(setIsProcessing).toHaveBeenCalledWith(false)
    expect(setErrorMessage).toHaveBeenCalledWith('connection refused')
    expect(setRecentRuns).toHaveBeenCalledOnce()
  })

  it('dispatches artifact.list to setScreenshotFiles', () => {
    const setScreenshotFiles = vi.fn()
    const setters = makeSetters({ setScreenshotFiles })
    handleWsMessage(
      {
        type: 'artifact.list',
        data: {
          ctx: { exec_id: 'e-1', mode: 'crawl', input: 'test' },
          artifacts: [{ path: 'screenshots/a.png', download_url: '/dl/a.png', size_bytes: 100 }],
        },
      } as WsServerMsg,
      makeRefs(),
      setters,
    )
    expect(setScreenshotFiles).toHaveBeenCalledOnce()
  })

  it('dispatches artifact.content — skips for scrape/crawl/extract without selected file', () => {
    const setMarkdownContent = vi.fn()
    const refs = makeRefs()
    refs.currentModeRef.current = 'scrape'
    refs.selectedFileRef.current = null
    const setters = makeSetters({ setMarkdownContent })
    handleWsMessage(
      {
        type: 'artifact.content',
        data: {
          ctx: { exec_id: 'e-1', mode: 'scrape', input: 'test' },
          path: 'index.md',
          content: '# Hello',
        },
      } as WsServerMsg,
      refs,
      setters,
    )
    expect(setMarkdownContent).not.toHaveBeenCalled()
  })

  it('dispatches artifact.content — sets content when file is selected', () => {
    const setMarkdownContent = vi.fn()
    const setHasResults = vi.fn()
    const refs = makeRefs()
    refs.currentModeRef.current = 'crawl'
    refs.selectedFileRef.current = 'index.md'
    const setters = makeSetters({ setMarkdownContent, setHasResults })
    handleWsMessage(
      {
        type: 'artifact.content',
        data: {
          ctx: { exec_id: 'e-1', mode: 'crawl', input: 'test' },
          path: 'index.md',
          content: '# Hello',
        },
      } as WsServerMsg,
      refs,
      setters,
    )
    expect(setMarkdownContent).toHaveBeenCalledWith('# Hello')
    expect(setHasResults).toHaveBeenCalledWith(true)
  })

  it('dispatches job.cancel.response to setCancelResponse and setLogLines', () => {
    const setCancelResponse = vi.fn()
    const setLogLines = vi.fn()
    const setters = makeSetters({ setCancelResponse, setLogLines })
    handleWsMessage(
      {
        type: 'job.cancel.response',
        data: {
          ctx: { exec_id: 'e-1', mode: 'crawl', input: 'test' },
          payload: { ok: true, message: 'Canceled', mode: 'crawl', job_id: 'j-1' },
        },
      } as WsServerMsg,
      makeRefs(),
      setters,
    )
    expect(setCancelResponse).toHaveBeenCalledOnce()
    expect(setLogLines).toHaveBeenCalledOnce()
  })

  it('dispatches stats as no-op', () => {
    const setters = makeSetters()
    handleWsMessage({ type: 'stats' } as WsServerMsg, makeRefs(), setters)
    // None of the critical setters should be called
    expect(setters.setLogLines).not.toHaveBeenCalled()
    expect(setters.setMarkdownContent).not.toHaveBeenCalled()
    expect(setters.setIsProcessing).not.toHaveBeenCalled()
  })

  it('handles command.output.json for scrape mode — creates virtual file', () => {
    const setVirtualFileContentByPath = vi.fn()
    const setCrawlFiles = vi.fn()
    const refs = makeRefs()
    refs.currentInputRef.current = 'https://example.com/page'
    const setters = makeSetters({ setVirtualFileContentByPath, setCrawlFiles })
    handleWsMessage(
      {
        type: 'command.output.json',
        data: {
          ctx: { exec_id: 'e-1', mode: 'scrape', input: 'https://example.com/page' },
          data: { url: 'https://example.com/page', markdown: '# Scraped content' },
        },
      } as WsServerMsg,
      refs,
      setters,
    )
    expect(setVirtualFileContentByPath).toHaveBeenCalledOnce()
    expect(setCrawlFiles).toHaveBeenCalledOnce()
  })
})
