'use client'

import type { Dispatch, SetStateAction } from 'react'
import type { CrawlFile, WsLifecycleEntry, WsServerMsg } from '@/lib/ws-protocol'
import {
  buildWorkspaceHandoffPrompt,
  MAX_LOG_LINES,
  pushCapped,
  reduceRuntimeState,
  setStatusResultLine,
  summarizeJsonValue,
} from './runtime'
import type {
  CancelResponseState,
  CrawlProgress,
  LogLine,
  RecentRun,
  ScreenshotFile,
  WsMessagesRuntimeState,
} from './types'

export interface MessageHandlerRefs {
  currentModeRef: React.RefObject<string>
  currentInputRef: React.RefObject<string>
  currentJobIdRef: React.RefObject<string | null>
  selectedFileRef: React.RefObject<string | null>
  crawlFilesRef: React.RefObject<CrawlFile[]>
  stdoutJsonRef: React.RefObject<unknown[]>
  currentOutputDirRef: React.RefObject<string | null>
  virtualFileContentByPathRef: React.RefObject<Record<string, string>>
  runIdCounter: React.RefObject<number>
  /** Current snapshot of WsMessagesRuntimeState — kept in sync by the hook. */
  runtimeStateRef: React.RefObject<WsMessagesRuntimeState>
}

export interface MessageHandlerSetters {
  setLogLines: Dispatch<SetStateAction<LogLine[]>>
  setMarkdownContent: Dispatch<SetStateAction<string>>
  setHasResults: Dispatch<SetStateAction<boolean>>
  setCrawlFiles: Dispatch<SetStateAction<CrawlFile[]>>
  setCurrentOutputDir: Dispatch<SetStateAction<string | null>>
  setSelectedFile: Dispatch<SetStateAction<string | null>>
  setCrawlProgress: Dispatch<SetStateAction<CrawlProgress | null>>
  setCommandMode: Dispatch<SetStateAction<string | null>>
  setStdoutLines: Dispatch<SetStateAction<string[]>>
  setStdoutJson: Dispatch<SetStateAction<unknown[]>>
  setVirtualFileContentByPath: Dispatch<SetStateAction<Record<string, string>>>
  setScreenshotFiles: Dispatch<SetStateAction<ScreenshotFile[]>>
  setLifecycleEntries: Dispatch<SetStateAction<WsLifecycleEntry[]>>
  setCancelResponse: Dispatch<SetStateAction<CancelResponseState | null>>
  setIsProcessing: Dispatch<SetStateAction<boolean>>
  setErrorMessage: Dispatch<SetStateAction<string>>
  setRecentRuns: Dispatch<SetStateAction<RecentRun[]>>
  setWorkspaceMode: Dispatch<SetStateAction<string | null>>
  setWorkspacePrompt: Dispatch<SetStateAction<string | null>>
  setWorkspacePromptVersion: Dispatch<SetStateAction<number>>
  setCurrentJobIdTracked: (jobId: string | null) => void
}

/** Canonical list of WS event types that the runtime message handler cares about.
 * Imported by subscription.ts as the subscription filter — single source of truth. */
export const RUNTIME_EVENT_TYPES: ReadonlyArray<WsServerMsg['type']> = [
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
]

const RUNTIME_MESSAGE_TYPES = new Set<WsServerMsg['type']>(RUNTIME_EVENT_TYPES)

export function isRuntimeRelevantWsMessage(msg: WsServerMsg): boolean {
  return RUNTIME_MESSAGE_TYPES.has(msg.type)
}

// ── Runtime state flush ──────────────────────────────────────────────────────

/**
 * Apply the fields from a WsMessagesRuntimeState snapshot to the individual React
 * state setters. Called after reduceRuntimeState() produces a new snapshot so
 * that handleWsMessage delegates runtime-slice logic to the reducer rather than
 * duplicating it.
 *
 * Only fields that actually changed are dispatched to avoid extra re-renders.
 */
function flushRuntimeState(
  prev: WsMessagesRuntimeState,
  next: WsMessagesRuntimeState,
  setters: MessageHandlerSetters,
  refs: MessageHandlerRefs,
): void {
  if (next.currentJobId !== prev.currentJobId) {
    setters.setCurrentJobIdTracked(next.currentJobId)
  }
  if (next.commandMode !== prev.commandMode) {
    setters.setCommandMode(next.commandMode)
  }
  if (next.markdownContent !== prev.markdownContent) {
    setters.setMarkdownContent(next.markdownContent)
  }
  if (next.crawlProgress !== prev.crawlProgress) {
    setters.setCrawlProgress(next.crawlProgress)
  }
  if (next.screenshotFiles !== prev.screenshotFiles) {
    setters.setScreenshotFiles(next.screenshotFiles)
  }
  if (next.lifecycleEntries !== prev.lifecycleEntries) {
    setters.setLifecycleEntries(next.lifecycleEntries)
  }
  if (next.stdoutJson !== prev.stdoutJson) {
    setters.setStdoutJson(next.stdoutJson)
    // Note: stdoutJsonRef.current is also updated by setStdoutJsonTracked in the hook.
  }
  if (next.cancelResponse !== prev.cancelResponse) {
    setters.setCancelResponse(next.cancelResponse)
  }
  // Keep runtimeStateRef in sync so subsequent messages see updated state.
  refs.runtimeStateRef.current = next
}

// ── Helpers ─────────────────────────────────────────────────────────────────

function buildRecentRun(
  runIdCounter: React.RefObject<number>,
  status: 'done' | 'failed',
  mode: string,
  target: string,
  elapsedMs?: number,
): RecentRun {
  return {
    id: `run-${++runIdCounter.current}`,
    status,
    mode,
    target,
    duration: `${((elapsedMs ?? 0) / 1000).toFixed(1)}s`,
    lines: 0,
    time: new Date().toLocaleTimeString(),
  }
}

function prependRecentRun(setRecentRuns: Dispatch<SetStateAction<RecentRun[]>>, run: RecentRun) {
  setRecentRuns((prev) => [run, ...prev].slice(0, 20))
}

// ── Per-type handlers ───────────────────────────────────────────────────────

function handleCommandOutputJson(
  msg: Extract<WsServerMsg, { type: 'command.output.json' }>,
  refs: MessageHandlerRefs,
  setters: MessageHandlerSetters,
) {
  // currentJobId and stdoutJson are already flushed via reduceRuntimeState in
  // handleWsMessage before this function is called — do not duplicate them here.
  const { currentInputRef, virtualFileContentByPathRef } = refs
  const { setCrawlFiles, setCurrentOutputDir, setHasResults, setVirtualFileContentByPath } = setters

  const maybeJobData =
    msg.data.data && typeof msg.data.data === 'object' && !Array.isArray(msg.data.data)
      ? (msg.data.data as Record<string, unknown>)
      : null
  if (maybeJobData && typeof maybeJobData.output_dir === 'string') {
    setCurrentOutputDir(maybeJobData.output_dir)
  }
  if (msg.data.ctx.mode === 'scrape' && maybeJobData) {
    const markdown = typeof maybeJobData.markdown === 'string' ? maybeJobData.markdown : null
    const url = typeof maybeJobData.url === 'string' ? maybeJobData.url : currentInputRef.current
    if (markdown && markdown.length > 0) {
      const basename = url.replace(/^https?:\/\//i, '').replace(/[^a-z0-9]+/gi, '-')
      const relativePath = `virtual/scrape-${basename || 'result'}.md`
      virtualFileContentByPathRef.current = {
        ...virtualFileContentByPathRef.current,
        [relativePath]: markdown,
      }
      setVirtualFileContentByPath((prev) => ({
        ...prev,
        [relativePath]: markdown,
      }))
      setCrawlFiles((prev) => {
        const withoutExisting = prev.filter((f) => f.relative_path !== relativePath)
        return [
          ...withoutExisting,
          { url, relative_path: relativePath, markdown_chars: markdown.length },
        ]
      })
    }
  }
  if (msg.data.ctx.mode === 'extract' && maybeJobData) {
    const relativePath = 'virtual/extract-result.json'
    const serialized = summarizeJsonValue(maybeJobData)
    setVirtualFileContentByPath((prev) => ({
      ...prev,
      [relativePath]: serialized,
    }))
    setCrawlFiles((prev) => {
      const withoutExisting = prev.filter((f) => f.relative_path !== relativePath)
      return [
        ...withoutExisting,
        {
          url: currentInputRef.current || 'extract://result',
          relative_path: relativePath,
          markdown_chars: serialized.length,
        },
      ]
    })
  }
  setHasResults(true)
}

function handleCommandDone(
  msg: Extract<WsServerMsg, { type: 'command.done' }>,
  refs: MessageHandlerRefs,
  setters: MessageHandlerSetters,
) {
  const {
    currentModeRef,
    currentInputRef,
    crawlFilesRef,
    stdoutJsonRef,
    currentOutputDirRef,
    virtualFileContentByPathRef,
    runIdCounter,
  } = refs
  const {
    setIsProcessing,
    setHasResults,
    setRecentRuns,
    setWorkspaceMode,
    setWorkspacePrompt,
    setWorkspacePromptVersion,
  } = setters

  setIsProcessing(false)
  if (
    msg.data.payload.exit_code === 0 &&
    (currentModeRef.current === 'scrape' ||
      currentModeRef.current === 'crawl' ||
      currentModeRef.current === 'extract')
  ) {
    const handoffPrompt = buildWorkspaceHandoffPrompt({
      modeLabel: currentModeRef.current,
      filesSnapshot: crawlFilesRef.current,
      targetInput: currentInputRef.current.trim(),
      outputDir: currentOutputDirRef.current,
      stdoutSnapshot: stdoutJsonRef.current,
      virtualFileContentByPath: virtualFileContentByPathRef.current,
    })
    setWorkspaceMode('pulse')
    setHasResults(true)
    setWorkspacePrompt(handoffPrompt)
    setWorkspacePromptVersion((prev) => prev + 1)
  }
  const run = buildRecentRun(
    runIdCounter,
    msg.data.payload.exit_code === 0 ? 'done' : 'failed',
    currentModeRef.current,
    currentInputRef.current,
    msg.data.payload.elapsed_ms,
  )
  prependRecentRun(setRecentRuns, run)
}

function handleCommandError(
  msg: Extract<WsServerMsg, { type: 'command.error' }>,
  refs: MessageHandlerRefs,
  setters: MessageHandlerSetters,
) {
  const { currentModeRef, currentInputRef, runIdCounter } = refs
  const { setIsProcessing, setErrorMessage, setRecentRuns } = setters

  setIsProcessing(false)
  setErrorMessage(msg.data.payload.message)
  const run = buildRecentRun(
    runIdCounter,
    'failed',
    currentModeRef.current,
    currentInputRef.current,
    msg.data.payload.elapsed_ms,
  )
  prependRecentRun(setRecentRuns, run)
}

// ── Main dispatcher ─────────────────────────────────────────────────────────

export function handleWsMessage(
  msg: WsServerMsg,
  refs: MessageHandlerRefs,
  setters: MessageHandlerSetters,
): void {
  if (msg.type === 'stats') return

  // Delegate runtime-slice state to reduceRuntimeState, then flush changes.
  // This keeps reduceRuntimeState as the single source of truth for the fields
  // in WsMessagesRuntimeState — no logic is duplicated here.
  //
  // Exception: artifact.content has a handler-level guard (skip auto-set when
  // in scrape/crawl/extract mode without an explicit file selection) that cannot
  // be expressed in the pure reducer without access to refs, so it is handled
  // inline below and the reducer case is bypassed for that message type.
  if (msg.type !== 'artifact.content') {
    const prev = refs.runtimeStateRef.current
    const next = reduceRuntimeState(prev, msg)
    if (next !== prev) {
      flushRuntimeState(prev, next, setters, refs)
    }
  }

  switch (msg.type) {
    case 'log':
      setters.setLogLines((prev) =>
        pushCapped(prev, { content: msg.line, timestamp: Date.now() }, MAX_LOG_LINES),
      )
      break
    case 'file_content':
      // file_content is a legacy message type not handled by reduceRuntimeState.
      // Sync runtimeStateRef manually so subsequent messages see up-to-date markdownContent.
      setters.setMarkdownContent(msg.content)
      refs.runtimeStateRef.current = {
        ...refs.runtimeStateRef.current,
        markdownContent: msg.content,
      }
      setters.setHasResults(true)
      break
    case 'crawl_files':
      setters.setCrawlFiles(msg.files)
      setters.setCurrentOutputDir(msg.output_dir)
      setters.setHasResults(true)
      // job_id and selectedFile guards are outside the runtime slice — stay here.
      setters.setCurrentJobIdTracked(msg.job_id ?? null)
      setters.setSelectedFile((prev) =>
        prev && msg.files.some((file) => file.relative_path === prev) ? prev : null,
      )
      break
    case 'crawl_progress':
      // crawl_progress runtime fields (crawlProgress) are handled via reduceRuntimeState above.
      // currentJobId from msg.job_id is outside the reducer path for this message type.
      if (msg.job_id) setters.setCurrentJobIdTracked(msg.job_id)
      break
    case 'command.start':
      // commandMode and stdoutJson are flushed via reduceRuntimeState above.
      // stdoutLines is outside the runtime slice — clear it here.
      setters.setStdoutLines([])
      break
    case 'command.output.json':
      // currentJobId and stdoutJson are flushed via reduceRuntimeState above.
      // scrape/extract virtual file handling, output_dir, and hasResults are
      // outside the runtime slice — delegate to the dedicated handler.
      handleCommandOutputJson(msg, refs, setters)
      break
    case 'command.output.line':
      setters.setStdoutLines((prev) => pushCapped(prev, msg.data.line))
      setters.setHasResults(true)
      break
    case 'job.status':
      // currentJobId, lifecycleEntries, stdoutJson flushed via reduceRuntimeState above.
      setters.setHasResults(true)
      break
    case 'job.progress':
      // lifecycleEntries, stdoutJson flushed via reduceRuntimeState above.
      setters.setHasResults(true)
      break
    case 'artifact.list':
      // screenshotFiles flushed via reduceRuntimeState above.
      setters.setHasResults(true)
      break
    case 'artifact.content': {
      // Handler-level guard: in scrape/crawl/extract mode, only set markdownContent
      // when the user has explicitly selected a file. The pure reducer cannot express
      // this without access to refs, so we bypass the reducer for this message type
      // (see the pre-switch block above) and handle it inline.
      if (
        (refs.currentModeRef.current === 'scrape' ||
          refs.currentModeRef.current === 'crawl' ||
          refs.currentModeRef.current === 'extract') &&
        !refs.selectedFileRef.current
      ) {
        break
      }
      setters.setMarkdownContent(msg.data.content)
      refs.runtimeStateRef.current = {
        ...refs.runtimeStateRef.current,
        markdownContent: msg.data.content,
      }
      setters.setHasResults(true)
      break
    }
    case 'job.cancel.response':
      // cancelResponse flushed via reduceRuntimeState above.
      // Log line side-effect is outside the runtime slice — stays here.
      setStatusResultLine(setters.setLogLines, msg.data.payload.ok, msg.data.payload.message)
      break
    case 'command.done':
      handleCommandDone(msg, refs, setters)
      break
    case 'command.error':
      handleCommandError(msg, refs, setters)
      break
  }
}
