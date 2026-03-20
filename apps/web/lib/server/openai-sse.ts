/** Shared OpenAI SSE stream parsing utilities. */

export interface CopilotStreamEvent {
  completion?: string
  delta?: string
  type: 'delta' | 'done' | 'error' | 'start'
}

export const encodeCopilotStreamEvent = (event: CopilotStreamEvent) => `${JSON.stringify(event)}\n`

/** Parse OpenAI SSE streaming chunks into content deltas. */
export function parseOpenAiSseChunk(
  chunk: string,
  remainder: string,
): { deltas: string[]; done: boolean; remainder: string } {
  const combined = remainder + chunk
  const lines = combined.split('\n')
  const nextRemainder = lines.pop() ?? ''
  const deltas: string[] = []
  let done = false

  for (const rawLine of lines) {
    const line = rawLine.trim()
    if (!line || !line.startsWith('data:')) continue
    const data = line.slice('data:'.length).trim()
    if (data === '[DONE]') {
      done = true
      break
    }
    try {
      const parsed = JSON.parse(data)
      const delta = parsed?.choices?.[0]?.delta?.content
      if (typeof delta === 'string' && delta.length > 0) {
        deltas.push(delta)
      }
    } catch {
      // Ignore malformed SSE lines.
    }
  }

  return { deltas, done, remainder: nextRemainder }
}
