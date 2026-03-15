import { z } from 'zod'
import { parseOpenAiSseChunk } from '@/app/api/ai/copilot/route'
import { ensureRepoRootEnvLoaded } from '@/lib/pulse/server-env'
import { apiError } from '@/lib/server/api-error'
import { logError } from '@/lib/server/logger'

const AIChatRequestSchema = z.object({
  prompt: z.string().min(1).max(16_000),
  model: z.string().optional(),
  ctx: z.record(z.string(), z.unknown()).optional(),
})

export async function POST(request: Request) {
  ensureRepoRootEnvLoaded()

  const baseUrl = process.env.OPENAI_BASE_URL
  const apiKey = process.env.OPENAI_API_KEY
  const model = process.env.OPENAI_MODEL ?? 'gpt-4o-mini'

  if (!baseUrl || !apiKey) {
    const missing = [...(baseUrl ? [] : ['OPENAI_BASE_URL']), ...(apiKey ? [] : ['OPENAI_API_KEY'])]
    return apiError(503, `${missing.join(', ')} must be set`, { code: 'ai_chat_config' })
  }

  try {
    const body = await request.json()
    const parsed = AIChatRequestSchema.safeParse(body)
    if (!parsed.success) {
      return apiError(400, parsed.error.issues[0]?.message ?? 'Invalid request')
    }

    const { prompt, ctx } = parsed.data
    const controller = new AbortController()
    const timeout = setTimeout(() => controller.abort(), 60_000)

    const response = await fetch(`${baseUrl}/chat/completions`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${apiKey}`,
      },
      body: JSON.stringify({
        model: parsed.data.model ?? model,
        messages: [{ role: 'user' as const, content: prompt }],
        max_tokens: 2000,
        temperature: 0.7,
        stream: true,
        ...(ctx ? { metadata: ctx } : {}),
      }),
      signal: controller.signal,
    }).finally(() => clearTimeout(timeout))

    if (!response.ok) {
      return apiError(502, `LLM API error: ${response.status}`, { code: 'ai_chat_upstream' })
    }

    // Stream SSE directly back to client
    return new Response(
      new ReadableStream({
        async start(controller) {
          if (!response.body) {
            controller.close()
            return
          }

          const reader = response.body.getReader()
          const decoder = new TextDecoder()
          let remainder = ''

          try {
            while (true) {
              const { value, done } = await reader.read()
              if (done) break

              const parsed = parseOpenAiSseChunk(decoder.decode(value, { stream: true }), remainder)
              remainder = parsed.remainder

              for (const delta of parsed.deltas) {
                controller.enqueue(
                  new TextEncoder().encode(
                    `data: ${JSON.stringify({ choices: [{ delta: { content: delta } }] })}\n\n`,
                  ),
                )
              }

              if (parsed.done) {
                controller.enqueue(new TextEncoder().encode('data: [DONE]\n\n'))
                break
              }
            }
          } catch {
            controller.enqueue(new TextEncoder().encode('data: [DONE]\n\n'))
          } finally {
            controller.close()
          }
        },
      }),
      {
        headers: {
          'Content-Type': 'text/event-stream; charset=utf-8',
          'Cache-Control': 'no-store',
          'X-Accel-Buffering': 'no',
        },
      },
    )
  } catch (err) {
    logError('api.ai.chat.unhandled_error', {
      message: err instanceof Error ? err.message : String(err),
    })
    return apiError(500, 'AI chat request failed', { code: 'ai_chat_internal' })
  }
}
