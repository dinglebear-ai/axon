import { describe, expect, it, vi } from 'vitest'
import type { MessageHandlerRefs, MessageHandlerSetters } from '@/hooks/ws-messages/handlers'
import {
  RUNTIME_WS_MESSAGE_TYPES,
  subscribeRuntimeMessages,
} from '@/hooks/ws-messages/subscription'
import type { WsServerMsg } from '@/lib/ws-protocol'

function makeRefs(): MessageHandlerRefs {
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
    runtimeStateRef: {
      current: {
        currentJobId: null,
        commandMode: null,
        markdownContent: '',
        crawlProgress: null,
        screenshotFiles: [],
        lifecycleEntries: [],
        stdoutJson: [],
        cancelResponse: null,
      },
    },
  }
}

function makeSetters(): MessageHandlerSetters {
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
  }
}

describe('RUNTIME_WS_MESSAGE_TYPES', () => {
  it('contains the runtime message types used by the provider subscription', () => {
    expect(RUNTIME_WS_MESSAGE_TYPES).toEqual([
      'log',
      'file_content',
      'crawl_files',
      'crawl_progress',
      'command.start',
      'command.output.line',
      'command.output.json',
      'command.done',
      'command.error',
      'job.status',
      'job.progress',
      'artifact.list',
      'artifact.content',
      'job.cancel.response',
    ])
  })
})

describe('subscribeRuntimeMessages', () => {
  it('subscribes to runtime types and only dispatches relevant messages', () => {
    const unsubscribe = vi.fn()
    const subscribeByTypes = vi.fn((_types, callback: (msg: WsServerMsg) => void) => {
      callback({ type: 'log', line: 'hello' } as WsServerMsg)
      callback({ type: 'stats' } as WsServerMsg)
      return unsubscribe
    })
    const handleMessage = vi.fn()

    const result = subscribeRuntimeMessages({
      subscribeByTypes,
      refs: makeRefs(),
      setters: makeSetters(),
      handleMessage,
    })

    expect(subscribeByTypes).toHaveBeenCalledWith(RUNTIME_WS_MESSAGE_TYPES, expect.any(Function))
    expect(handleMessage).toHaveBeenCalledTimes(1)
    expect(handleMessage).toHaveBeenCalledWith(
      { type: 'log', line: 'hello' },
      expect.any(Object),
      expect.any(Object),
    )
    expect(result).toBe(unsubscribe)
  })
})
