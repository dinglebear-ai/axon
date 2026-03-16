import fs from 'node:fs/promises'
import { NextResponse } from 'next/server'
import { makeErrorId } from '@/lib/server/api-error'
import { logError, logWarn } from '@/lib/server/logger'
import { enforceRateLimit } from '@/lib/server/rate-limit'
import { getCachedSessions } from '@/lib/server/session-cache'
import { parseClaudeJsonl } from '@/lib/sessions/claude-jsonl-parser'
import { parseCodexJsonl } from '@/lib/sessions/codex-jsonl-parser'
import { parseGeminiJson } from '@/lib/sessions/gemini-json-parser'

const LIST_LIMIT = 20
const DETAIL_LIMIT = 200
const PER_AGENT_LIMIT = 30
const SESSION_CACHE_TTL_MS = 30_000

export async function GET(_request: Request, { params }: { params: Promise<{ id: string }> }) {
  const limited = enforceRateLimit('api.sessions.detail', _request, { max: 60, windowMs: 60_000 })
  if (limited) return limited

  const { id } = await params

  // Defense-in-depth: reject obviously invalid IDs before lookup.
  if (!/^[\w.@:-]{1,255}$/.test(id)) {
    return NextResponse.json(
      { error: 'bad request', code: 'INVALID_SESSION_ID', errorId: makeErrorId('session') },
      { status: 400 },
    )
  }

  const url = new URL(_request.url)
  const assistantMode = url.searchParams.get('assistant_mode') === '1'
  const fresh = url.searchParams.get('fresh') === '1'
  const matchSession = (sessions: Awaited<ReturnType<typeof getCachedSessions>>) =>
    sessions.find((s) => s.id === id || s.filename === id)

  const fastSessions = await getCachedSessions({
    assistantMode,
    limit: LIST_LIMIT,
    perAgentLimit: PER_AGENT_LIMIT,
    ttlMs: SESSION_CACHE_TTL_MS,
    forceRefresh: fresh,
  })
  let session = matchSession(fastSessions)
  if (!session) {
    logWarn('api.sessions.detail.cache_miss_fallback_scan', { id, assistantMode })
    const fullSessions = await getCachedSessions({
      assistantMode,
      limit: DETAIL_LIMIT,
      perAgentLimit: PER_AGENT_LIMIT,
      ttlMs: SESSION_CACHE_TTL_MS,
      forceRefresh: fresh,
    })
    session = matchSession(fullSessions)
  }

  if (!session) {
    return NextResponse.json(
      {
        error: 'not found',
        code: 'SESSION_NOT_FOUND',
        errorId: makeErrorId('session'),
        detail: 'Session with provided id was not found',
      },
      { status: 404 },
    )
  }

  try {
    const raw = await fs.readFile(session.absolutePath, 'utf-8')
    const messages =
      session.agent === 'codex'
        ? parseCodexJsonl(raw)
        : session.agent === 'gemini'
          ? parseGeminiJson(raw)
          : parseClaudeJsonl(raw)
    return NextResponse.json({
      project: session.project,
      filename: session.filename,
      sessionId: session.filename,
      messages,
    })
  } catch (error: unknown) {
    logError('api.sessions.detail.read_failed', {
      id,
      assistantMode,
      message: error instanceof Error ? error.message : String(error),
    })
    return NextResponse.json({ error: 'read failed' }, { status: 500 })
  }
}
