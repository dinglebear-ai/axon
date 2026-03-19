import { normalizeLocalServiceUrl } from '@/lib/server/service-url'
import type { PulseChatRequest, PulseCitation } from './types'

interface QdrantPoint {
  payload?: {
    text?: string
    url?: string
    title?: string
    collection?: string
  }
  score?: number
}

interface QdrantSearchResponse {
  result?: QdrantPoint[]
}

function truncate(input: string, max = 300): string {
  if (input.length <= max) return input
  return `${input.slice(0, max)}...`
}

function excerptDocument(markdown: string, maxChars = 4000): string {
  return truncate(markdown, maxChars)
}

function buildConversationHistorySection(
  history: Array<{ role: 'user' | 'assistant'; content: string }>,
): string | null {
  if (history.length === 0) return null

  const maxTurns = 24
  const maxCharsPerTurn = 1200
  const maxTotalChars = 14_000
  const normalized = history
    .slice(-maxTurns)
    .map((turn) => {
      const role = turn.role === 'user' ? 'User' : 'Assistant'
      const content = truncate(turn.content.trim(), maxCharsPerTurn)
      return `${role}: ${content}`
    })
    .filter((line) => line.length > 0)

  if (normalized.length === 0) return null

  const lines: string[] = []
  let used = 0
  for (const line of normalized) {
    const projected = used + line.length + 1
    if (projected > maxTotalChars) break
    lines.push(line)
    used = projected
  }

  if (lines.length === 0) return null
  return lines.join('\n\n')
}

export async function retrieveFromCollections(
  query: string,
  selectedCollections: string[],
  limit = 4,
): Promise<PulseCitation[]> {
  const qdrantUrl = normalizeLocalServiceUrl(process.env.QDRANT_URL)
  const teiUrl = normalizeLocalServiceUrl(process.env.TEI_URL)
  if (!qdrantUrl || !teiUrl) return []

  try {
    const embedController = new AbortController()
    const embedTimeout = setTimeout(() => embedController.abort(), 20_000)
    const embedRes = await fetch(`${teiUrl}/embed`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ inputs: [query] }),
      signal: embedController.signal,
    }).finally(() => clearTimeout(embedTimeout))
    if (!embedRes.ok) return []

    const embedJson = (await embedRes.json()) as unknown
    const queryVector = Array.isArray(embedJson)
      ? (embedJson[0] as number[] | undefined)
      : undefined
    if (!queryVector) return []

    const perCollection = await Promise.all(
      selectedCollections.map(async (collection) => {
        try {
          const response = await fetch(
            `${qdrantUrl}/collections/${encodeURIComponent(collection)}/points/search`,
            {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({
                vector: queryVector,
                limit,
                with_payload: true,
              }),
            },
          )

          if (!response.ok) return [] as PulseCitation[]
          const data = (await response.json()) as QdrantSearchResponse
          return (data.result ?? []).map((point) => {
            const payload = point.payload ?? {}
            return {
              url: payload.url ?? '',
              title: payload.title ?? 'Untitled',
              snippet: truncate(payload.text ?? '', 280),
              collection,
              score: point.score ?? 0,
            } satisfies PulseCitation
          })
        } catch {
          return [] as PulseCitation[]
        }
      }),
    )

    return perCollection
      .flat()
      .sort((a, b) => b.score - a.score)
      .slice(0, limit * 2)
  } catch {
    return []
  }
}

export function buildPulseSystemPrompt(req: PulseChatRequest, citations: PulseCitation[]): string {
  const doc = excerptDocument(req.documentMarkdown)
  const conversationHistory = buildConversationHistorySection(req.conversationHistory)
  const citationContext = citations
    .map((c, i) => `(${i + 1}) [${c.collection}] ${c.title} ${c.url}\n${c.snippet}`)
    .join('\n\n')

  const parts: string[] = [
    'You are Cortex, a document copilot for editing markdown safely.',
    'Return plain assistant text and optional doc operations.',
    'Only suggest operations with these types: replace_document, append_markdown, insert_section.',
    `Permission level: ${req.permissionLevel}.`,
    `Model: ${req.model}.`,
    'Current document:',
    doc,
  ]

  // Scraped page content — injected directly (no Qdrant required).
  if (req.scrapedContext?.markdown) {
    parts.push(
      `Scraped page (${req.scrapedContext.url || 'unknown URL'}):`,
      truncate(req.scrapedContext.markdown, 8000),
    )
  }

  // Crawled sources — content indexed in cortex; RAG results appear below.
  if (req.threadSources.length > 0) {
    parts.push(
      'Crawled sources (content indexed in cortex collection — see Retrieved context below):',
      req.threadSources.join('\n'),
    )
  }

  if (conversationHistory) {
    parts.push(
      'Conversation history (oldest to newest, excluding the latest user message):',
      conversationHistory,
    )
  }

  parts.push('Retrieved context (semantic search over cortex):')
  parts.push(citationContext || 'No citations retrieved.')

  return parts.join('\n\n')
}
