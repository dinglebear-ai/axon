// Client → Server
export type WsClientMsg =
  | {
      type: 'execute'
      mode: string
      input: string
      flags: Record<string, string | boolean | number | string[]>
    }
  | { type: 'cancel'; id: string; mode?: string; job_id?: string }
  | { type: 'read_file'; path: string }
  | { type: 'acp_resume'; session_id: string }
  // TODO: Wire permission response to Rust AcpBridgeClient when leaving container mode
  | { type: 'permission_response'; tool_call_id: string; option_id: string }

// Server → Client
export type WsServerMsg =
  | { type: 'log'; line: string }
  | { type: 'file_content'; path: string; content: string }
  | { type: 'crawl_files'; files: CrawlFile[]; output_dir: string; job_id?: string }
  | {
      type: 'crawl_progress'
      job_id: string
      status: string
      pages_crawled: number
      pages_discovered: number
      md_created: number
      thin_md: number
      phase: string
    }
  | WsV2CommandOutputJsonMsg
  | WsV2CommandOutputLineMsg
  | WsV2CommandDoneMsg
  | WsV2CommandErrorMsg
  | {
      type: 'stats'
      aggregate: AggregateStats
      containers: Record<string, ContainerStats>
      container_count: number
    }
  | WsV2CommandStartMsg
  | WsV2JobStatusMsg
  | WsV2JobProgressMsg
  | WsV2ArtifactListMsg
  | WsV2ArtifactContentMsg
  | WsV2JobCancelResponseMsg
  | {
      type: 'assistant_delta'
      session_id?: string
      delta: string
      tool_call_id?: string | null
      tool_locations?: string[]
      usage?: WsUsageStats
    }
  | { type: 'usage_update'; session_id: string; usage: WsUsageStats }
  | { type: 'thinking_content'; session_id?: string; content: string; tool_call_id?: string | null }
  | { type: 'session_fallback'; old_session_id: string; new_session_id: string }
  | { type: 'result'; session_id?: string; result?: string; [key: string]: unknown }
  | { type: 'error'; message?: string; [key: string]: unknown }
  | { type: 'editor_update'; content: string; operation: 'replace' | 'append' }
  | {
      type: 'tool_use'
      tool_call_id?: string
      tool_name?: string
      tool_input?: Record<string, unknown>
    }
  | {
      type: 'tool_use_update'
      tool_call_id?: string
      tool_status?: string
      tool_content?: string
    }
  | { type: 'config_options_update'; configOptions: unknown[] }
  | { type: 'config_option_update'; configOptions: unknown[] }
  | {
      type: 'commands_update'
      commands: Array<{ name: string; description?: string }>
    }
  | {
      type: 'acp_resume_result'
      ok?: boolean
      replayed?: number
      session_id?: string
      reason?: string
    }
  | {
      type: 'permission_request'
      session_id: string
      request_id: string
      tool_name: string
      tool_input: unknown
    }

export interface WsV2CommandContext {
  exec_id: string
  mode: string
  input: string
}

export interface WsV2JobStatusPayload {
  status: string
  error?: string
  metrics?: Record<string, unknown>
}

export interface WsV2JobProgressPayload {
  phase: string
  percent?: number
  processed?: number
  total?: number
}

export interface WsV2ArtifactEntry {
  kind?: string
  path?: string
  download_url?: string
  mime?: string
  size_bytes?: number
}

export interface WsV2CommandStartMsg {
  type: 'command.start'
  data: {
    ctx: WsV2CommandContext
  }
}

// NOTE: These event types are defined for a future push-model implementation.
// As of 2026-03, async job progress is POLL-ONLY via GET /api/jobs/:id.
// The server never emits these WS events. Do not rely on them at runtime.
export interface WsV2JobStatusMsg {
  type: 'job.status'
  data: {
    ctx: WsV2CommandContext
    payload: WsV2JobStatusPayload
  }
}

// NOTE: These event types are defined for a future push-model implementation.
// As of 2026-03, async job progress is POLL-ONLY via GET /api/jobs/:id.
// The server never emits these WS events. Do not rely on them at runtime.
export interface WsV2JobProgressMsg {
  type: 'job.progress'
  data: {
    ctx: WsV2CommandContext
    payload: WsV2JobProgressPayload
  }
}

export interface WsV2CommandOutputJsonMsg {
  type: 'command.output.json'
  data: {
    ctx: WsV2CommandContext
    data: unknown
  }
}

export interface WsV2CommandOutputLineMsg {
  type: 'command.output.line'
  data: {
    ctx: WsV2CommandContext
    line: string
  }
}

export interface WsV2CommandDoneMsg {
  type: 'command.done'
  data: {
    ctx: WsV2CommandContext
    payload: {
      exit_code: number
      elapsed_ms?: number
    }
  }
}

export interface WsV2CommandErrorMsg {
  type: 'command.error'
  data: {
    ctx: WsV2CommandContext
    payload: {
      message: string
      elapsed_ms?: number
    }
  }
}

export interface WsV2ArtifactListMsg {
  type: 'artifact.list'
  data: {
    ctx: WsV2CommandContext
    artifacts: WsV2ArtifactEntry[]
  }
}

export interface WsV2ArtifactContentMsg {
  type: 'artifact.content'
  data: {
    ctx: WsV2CommandContext
    path: string
    content: string
  }
}

export interface WsV2JobCancelResponseMsg {
  type: 'job.cancel.response'
  data: {
    ctx: WsV2CommandContext
    payload: {
      ok: boolean
      mode?: string
      job_id?: string
      message?: string
    }
  }
}

export interface WsLifecycleEntry {
  job_id: string
  status?: string
  mode?: string
  phase?: string
  percent?: number
  processed?: number
  total?: number
  error_text?: string
}

function asFiniteNumber(value: unknown): number | undefined {
  if (typeof value !== 'number' || Number.isNaN(value) || !Number.isFinite(value)) return undefined
  return value
}

function asPositiveInteger(value: unknown): number | undefined {
  const num = asFiniteNumber(value)
  if (num === undefined) return undefined
  const int = Math.floor(num)
  return int >= 0 ? int : undefined
}

// NOTE: These event types are defined for a future push-model implementation.
// As of 2026-03, async job progress is POLL-ONLY via GET /api/jobs/:id.
// The server never emits these WS events. Do not rely on them at runtime.
export function lifecycleFromJobStatus(
  msg: WsV2JobStatusMsg,
  fallbackJobId: string | null,
): WsLifecycleEntry | null {
  const metrics = msg.data.payload.metrics
  const metricsRecord =
    metrics && typeof metrics === 'object' && !Array.isArray(metrics)
      ? (metrics as Record<string, unknown>)
      : null
  const metricsJobId =
    typeof metricsRecord?.job_id === 'string'
      ? metricsRecord.job_id
      : typeof metricsRecord?.id === 'string'
        ? metricsRecord.id
        : null
  const job_id = metricsJobId ?? fallbackJobId
  if (!job_id) return null

  const status = msg.data.payload.status
  const mode = msg.data.ctx.mode
  const error_text = msg.data.payload.error
  return {
    job_id,
    mode,
    status,
    error_text: error_text && error_text.length > 0 ? error_text : undefined,
  }
}

// NOTE: These event types are defined for a future push-model implementation.
// As of 2026-03, async job progress is POLL-ONLY via GET /api/jobs/:id.
// The server never emits these WS events. Do not rely on them at runtime.
export function lifecycleFromJobProgress(
  msg: WsV2JobProgressMsg,
  fallbackJobId: string | null,
): WsLifecycleEntry | null {
  const job_id = fallbackJobId
  if (!job_id) return null

  return {
    job_id,
    mode: msg.data.ctx.mode,
    status: 'running',
    phase: msg.data.payload.phase,
    percent: asFiniteNumber(msg.data.payload.percent),
    processed: asPositiveInteger(msg.data.payload.processed),
    total: asPositiveInteger(msg.data.payload.total),
  }
}

export interface CrawlFile {
  url: string
  relative_path: string
  markdown_chars: number
}

export interface AggregateStats {
  cpu_percent: number
  avg_memory_percent: number
  total_net_io_rate: number
}

export interface ContainerStats {
  cpu_percent: number
  memory_usage_mb: number
  memory_limit_mb: number
  net_rx_rate: number
  net_tx_rate: number
}

export interface WsUsageStats {
  input_tokens: number
  output_tokens: number
  total_tokens: number
  cache_creation_input_tokens?: number
  cache_read_input_tokens?: number
}

export type WsStatus = 'connected' | 'reconnecting' | 'disconnected'

// Mode definitions — must match ALLOWED_MODES in crates/web/execute.rs
// Grouped by AxonCommandCategory for the mode picker dropdown.

export type ModeCategory = 'content' | 'rag' | 'ingest' | 'ops' | 'service'

export interface ModeDefinition {
  id: string
  label: string
  icon: string
  category: ModeCategory
}

export const MODES: readonly ModeDefinition[] = [
  // --- content ---
  {
    id: 'scrape',
    label: 'Scrape',
    category: 'content',
    icon: 'M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z',
  },
  {
    id: 'crawl',
    label: 'Crawl',
    category: 'content',
    icon: 'M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9',
  },
  {
    id: 'map',
    label: 'Map',
    category: 'content',
    icon: 'M9 20l-5.447-2.724A1 1 0 013 16.382V5.618a1 1 0 011.447-.894L9 7m0 13l6-3m-6 3V7m6 10l4.553 2.276A1 1 0 0021 18.382V7.618a1 1 0 00-.553-.894L15 4m0 13V4m0 0L9 7',
  },
  {
    id: 'extract',
    label: 'Extract',
    category: 'content',
    icon: 'M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M9 19l3 3m0 0l3-3m-3 3V10',
  },
  {
    id: 'screenshot',
    label: 'Screenshot',
    category: 'content',
    icon: 'M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z M15 13a3 3 0 11-6 0 3 3 0 016 0z',
  },

  // --- rag ---
  {
    id: 'search',
    label: 'Search',
    category: 'rag',
    icon: 'M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0zM10 7v3m0 0v3m0-3h3m-3 0H7',
  },
  {
    id: 'research',
    label: 'Research',
    category: 'rag',
    icon: 'M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253',
  },
  {
    id: 'query',
    label: 'Query',
    category: 'rag',
    icon: 'M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z',
  },
  {
    id: 'retrieve',
    label: 'Retrieve',
    category: 'rag',
    icon: 'M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4',
  },
  {
    id: 'ask',
    label: 'Ask',
    category: 'rag',
    icon: 'M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z',
  },
  {
    id: 'evaluate',
    label: 'Evaluate',
    category: 'rag',
    icon: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4',
  },
  {
    id: 'suggest',
    label: 'Suggest',
    category: 'rag',
    icon: 'M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z',
  },

  // --- ingest ---
  {
    id: 'github',
    label: 'GitHub',
    category: 'ingest',
    icon: 'M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 00-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0020 4.77 5.07 5.07 0 0019.91 1S18.73.65 16 2.48a13.38 13.38 0 00-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 005 4.77a5.44 5.44 0 00-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 009 18.13V22',
  },
  {
    id: 'reddit',
    label: 'Reddit',
    category: 'ingest',
    icon: 'M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10zm4.5-12.5a1.5 1.5 0 110 3 1.5 1.5 0 010-3zm-9 0a1.5 1.5 0 110 3 1.5 1.5 0 010-3zM8.5 16.5s1.5 2 3.5 2 3.5-2 3.5-2',
  },
  {
    id: 'youtube',
    label: 'YouTube',
    category: 'ingest',
    icon: 'M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
  },
  {
    id: 'sessions',
    label: 'Sessions',
    category: 'ingest',
    icon: 'M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z',
  },
  // --- ops ---
  {
    id: 'embed',
    label: 'Embed',
    category: 'ops',
    icon: 'M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10',
  },
  {
    id: 'sources',
    label: 'Sources',
    category: 'ops',
    icon: 'M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4',
  },
  {
    id: 'domains',
    label: 'Domains',
    category: 'ops',
    icon: 'M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064',
  },
  {
    id: 'stats',
    label: 'Stats',
    category: 'ops',
    icon: 'M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z',
  },
  { id: 'status', label: 'Status', category: 'ops', icon: 'M13 10V3L4 14h7v7l9-11h-7z' },
  {
    id: 'dedupe',
    label: 'Dedupe',
    category: 'ops',
    icon: 'M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16',
  },

  // --- service ---
  {
    id: 'pulse_chat',
    label: 'Pulse Chat',
    category: 'service',
    icon: 'M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z',
  },
  {
    id: 'doctor',
    label: 'Doctor',
    category: 'service',
    icon: 'M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z',
  },
  {
    id: 'debug',
    label: 'Debug',
    category: 'service',
    icon: 'M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
  },
] as const

export type ModeId = (typeof MODES)[number]['id']

/** Category labels for the mode picker dropdown. */
export const MODE_CATEGORY_LABELS: Record<ModeCategory, string> = {
  content: 'Content',
  rag: 'RAG',
  ingest: 'Ingest',
  ops: 'Ops',
  service: 'Service',
}

/** Category display order. */
export const MODE_CATEGORY_ORDER: readonly ModeCategory[] = [
  'content',
  'rag',
  'ingest',
  'ops',
  'service',
]

// Modes that auto-execute without input.
export const NO_INPUT_MODES: ReadonlySet<string> = new Set([
  'stats',
  'status',
  'doctor',
  'debug',
  'domains',
  'sources',
  'suggest',
  'sessions',
  'dedupe',
])
