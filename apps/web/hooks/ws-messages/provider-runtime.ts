import type { Dispatch, MutableRefObject, SetStateAction } from 'react'
import { useCallback, useMemo, useRef, useState } from 'react'
import type { CrawlFile, WsLifecycleEntry } from '@/lib/ws-protocol'
import { makeInitialRuntimeState } from './runtime'
import { createTrackedSetter } from './tracked'
import type {
  CancelResponseState,
  CrawlProgress,
  LogLine,
  RecentRun,
  ScreenshotFile,
  WsMessagesRuntimeState,
} from './types'

export interface WsProviderRuntimeRefs {
  currentModeRef: MutableRefObject<string>
  currentInputRef: MutableRefObject<string>
  currentJobIdRef: MutableRefObject<string | null>
  selectedFileRef: MutableRefObject<string | null>
  crawlFilesRef: MutableRefObject<CrawlFile[]>
  stdoutJsonRef: MutableRefObject<unknown[]>
  currentOutputDirRef: MutableRefObject<string | null>
  virtualFileContentByPathRef: MutableRefObject<Record<string, string>>
  runIdCounter: MutableRefObject<number>
  runtimeStateRef: MutableRefObject<WsMessagesRuntimeState>
}

export interface WsProviderRuntimeSetters {
  setMarkdownContent: Dispatch<SetStateAction<string>>
  setLogLines: Dispatch<SetStateAction<LogLine[]>>
  setErrorMessage: Dispatch<SetStateAction<string>>
  setRecentRuns: Dispatch<SetStateAction<RecentRun[]>>
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
}

export interface WsProviderRuntimeState {
  markdownContent: string
  logLines: LogLine[]
  errorMessage: string
  recentRuns: RecentRun[]
  isProcessing: boolean
  hasResults: boolean
  currentMode: string
  crawlFiles: CrawlFile[]
  selectedFile: string | null
  crawlProgress: CrawlProgress | null
  stdoutLines: string[]
  stdoutJson: unknown[]
  commandMode: string | null
  screenshotFiles: ScreenshotFile[]
  currentJobId: string | null
  lifecycleEntries: WsLifecycleEntry[]
  cancelResponse: CancelResponseState | null
}

export interface WsProviderRuntime {
  state: WsProviderRuntimeState
  refs: WsProviderRuntimeRefs
  setters: WsProviderRuntimeSetters
}

export function useWsProviderRuntime(): WsProviderRuntime {
  const [markdownContent, setMarkdownContent] = useState('')
  const [logLines, setLogLines] = useState<LogLine[]>([])
  const [errorMessage, setErrorMessage] = useState('')
  const [recentRuns, setRecentRuns] = useState<RecentRun[]>([])
  const runIdCounter = useRef(0)
  const [isProcessing, setIsProcessing] = useState(false)
  const [hasResults, setHasResults] = useState(false)
  const [crawlFiles, setCrawlFiles] = useState<CrawlFile[]>([])
  const [selectedFile, setSelectedFile] = useState<string | null>(null)
  const [, setVirtualFileContentByPath] = useState<Record<string, string>>({})
  const [, setCurrentOutputDir] = useState<string | null>(null)
  const currentModeRef = useRef('')
  const currentInputRef = useRef('')
  const [currentMode, setCurrentMode] = useState('')
  const [crawlProgress, setCrawlProgress] = useState<CrawlProgress | null>(null)
  const [stdoutLines, setStdoutLines] = useState<string[]>([])
  const [stdoutJson, setStdoutJson] = useState<unknown[]>([])
  const [commandMode, setCommandMode] = useState<string | null>(null)
  const [screenshotFiles, setScreenshotFiles] = useState<ScreenshotFile[]>([])
  const [currentJobId, setCurrentJobId] = useState<string | null>(null)
  const currentJobIdRef = useRef<string | null>(null)
  const [lifecycleEntries, setLifecycleEntries] = useState<WsLifecycleEntry[]>([])
  const [cancelResponse, setCancelResponse] = useState<CancelResponseState | null>(null)

  const selectedFileRef = useRef<string | null>(null)
  const crawlFilesRef = useRef<CrawlFile[]>([])
  const stdoutJsonRef = useRef<unknown[]>([])
  const currentOutputDirRef = useRef<string | null>(null)
  const virtualFileContentByPathRef = useRef<Record<string, string>>({})
  const runtimeStateRef = useRef<WsMessagesRuntimeState>(makeInitialRuntimeState())

  const setCrawlFilesTracked = useCallback(createTrackedSetter(setCrawlFiles, crawlFilesRef), [])
  const setSelectedFileTracked = useCallback(
    createTrackedSetter(setSelectedFile, selectedFileRef),
    [],
  )
  const setStdoutJsonTracked = useCallback(createTrackedSetter(setStdoutJson, stdoutJsonRef), [])
  const setCurrentOutputDirTracked = useCallback(
    createTrackedSetter(setCurrentOutputDir, currentOutputDirRef),
    [],
  )
  const setVirtualFileContentByPathTracked = useCallback(
    createTrackedSetter(setVirtualFileContentByPath, virtualFileContentByPathRef),
    [],
  )
  const setCurrentJobIdTracked = useCallback((jobId: string | null) => {
    currentJobIdRef.current = jobId
    setCurrentJobId(jobId)
  }, [])

  const state = useMemo(
    () => ({
      markdownContent,
      logLines,
      errorMessage,
      recentRuns,
      isProcessing,
      hasResults,
      currentMode,
      crawlFiles,
      selectedFile,
      crawlProgress,
      stdoutLines,
      stdoutJson,
      commandMode,
      screenshotFiles,
      currentJobId,
      lifecycleEntries,
      cancelResponse,
    }),
    [
      markdownContent,
      logLines,
      errorMessage,
      recentRuns,
      isProcessing,
      hasResults,
      currentMode,
      crawlFiles,
      selectedFile,
      crawlProgress,
      stdoutLines,
      stdoutJson,
      commandMode,
      screenshotFiles,
      currentJobId,
      lifecycleEntries,
      cancelResponse,
    ],
  )

  const refs = useMemo(
    () => ({
      currentModeRef,
      currentInputRef,
      currentJobIdRef,
      selectedFileRef,
      crawlFilesRef,
      stdoutJsonRef,
      currentOutputDirRef,
      virtualFileContentByPathRef,
      runIdCounter,
      runtimeStateRef,
    }),
    [],
  )

  const setters = useMemo(
    () => ({
      setMarkdownContent,
      setLogLines,
      setErrorMessage,
      setRecentRuns,
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
    }),
    [
      setCrawlFilesTracked,
      setSelectedFileTracked,
      setStdoutJsonTracked,
      setCurrentJobIdTracked,
      setCurrentOutputDirTracked,
      setVirtualFileContentByPathTracked,
    ],
  )

  return { state, refs, setters }
}
