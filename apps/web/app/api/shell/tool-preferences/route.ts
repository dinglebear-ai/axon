import { NextResponse } from 'next/server'
import { z } from 'zod'
import { logError } from '@/lib/server/logger'
import {
  loadToolPreferences,
  saveToolPreferences,
  type ToolPreferencesRecord,
} from '@/lib/server/tool-preferences-store'

const ToolPresetSchema = z.object({
  id: z.string().min(1).max(120),
  name: z.string().min(1).max(120),
  enabledMcpServers: z.array(z.string().min(1).max(200)).max(200),
  enabledMcpTools: z.array(z.string().min(1).max(300)).max(500),
})

const ToolPreferencesSchema = z.object({
  enabledMcpServers: z.array(z.string().min(1).max(200)).max(200),
  enabledMcpTools: z.array(z.string().min(1).max(300)).max(500),
  presets: z.array(ToolPresetSchema).max(50),
})

export async function GET() {
  try {
    const prefs = await loadToolPreferences()
    return NextResponse.json(prefs)
  } catch (error) {
    logError('api.shell.tool_preferences.get_failed', {
      message: error instanceof Error ? error.message : String(error),
    })
    return NextResponse.json({ error: 'failed_to_load' }, { status: 500 })
  }
}

export async function PUT(request: Request) {
  let body: unknown
  try {
    body = await request.json()
  } catch {
    return NextResponse.json({ error: 'invalid_json' }, { status: 400 })
  }
  try {
    const parsed = ToolPreferencesSchema.safeParse(body)
    if (!parsed.success) {
      return NextResponse.json(
        { error: 'invalid_payload', details: parsed.error.flatten() },
        { status: 400 },
      )
    }
    const saved = await saveToolPreferences({
      ...parsed.data,
      updatedAt: new Date().toISOString(),
    } satisfies ToolPreferencesRecord)
    return NextResponse.json(saved)
  } catch (error) {
    logError('api.shell.tool_preferences.put_failed', {
      message: error instanceof Error ? error.message : String(error),
    })
    return NextResponse.json({ error: 'failed_to_save' }, { status: 500 })
  }
}
