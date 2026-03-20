import { createGateway } from '@ai-sdk/gateway'
import { generateText } from 'ai'
import type { NextRequest } from 'next/server'
import { NextResponse } from 'next/server'
import { apiError } from '@/lib/server/api-error'
import { encodeCopilotStreamEvent, parseOpenAiSseChunk } from '@/lib/server/openai-sse'

export type { CopilotStreamEvent } from '@/lib/server/openai-sse'
export { encodeCopilotStreamEvent, parseOpenAiSseChunk }

const DEFAULT_MODEL = 'gpt-4o-mini'
const ALLOWED_MODELS = new Set([DEFAULT_MODEL, 'gpt-4.1-mini'])

export async function POST(req: NextRequest) {
  try {
    const body = await req.json()
    const model = typeof body?.model === 'string' ? body.model : DEFAULT_MODEL
    const prompt = typeof body?.prompt === 'string' ? body.prompt.trim() : ''
    const system = typeof body?.system === 'string' ? body.system : undefined
    const streamNdjson = req.headers.get('x-copilot-stream') === '1'

    if (!ALLOWED_MODELS.has(model)) {
      return apiError(400, 'Unsupported model')
    }
    if (!prompt) {
      return apiError(400, 'prompt must be a non-empty string')
    }

    const apiKey = process.env.AI_GATEWAY_API_KEY
    if (!apiKey) {
      return apiError(401, 'Missing AI Gateway API key', { code: 'copilot_no_key' })
    }

    const gateway = createGateway({ apiKey })

    const result = await generateText({
      abortSignal: req.signal,
      maxOutputTokens: 50,
      model: gateway(`openai/${model}`),
      prompt,
      system,
      temperature: 0.7,
    })

    if (streamNdjson) {
      const completion = typeof result.text === 'string' ? result.text : ''
      const events = `${encodeCopilotStreamEvent({ type: 'start' })}${encodeCopilotStreamEvent({
        completion,
        type: 'done',
      })}`

      return new NextResponse(events, {
        headers: {
          'Cache-Control': 'no-store',
          'Content-Type': 'application/x-ndjson; charset=utf-8',
        },
        status: 200,
      })
    }

    return NextResponse.json(result)
  } catch (error) {
    if (error instanceof SyntaxError) {
      return apiError(400, 'Invalid JSON payload')
    }
    if (error instanceof Error && error.name === 'AbortError') {
      return apiError(408, 'Request timed out', { code: 'copilot_timeout' })
    }

    return apiError(500, 'Failed to process AI request', { code: 'copilot_internal' })
  }
}
