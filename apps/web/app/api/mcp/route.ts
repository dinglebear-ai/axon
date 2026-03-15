import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { type NextRequest, NextResponse } from 'next/server'
import { z } from 'zod'
import { logError } from '@/lib/server/logger'
import { validateStatusUrl } from './status/route'

const McpServerConfigSchema = z.object({
  command: z
    .string()
    // Allow only bare executable names; deny path separators and traversal.
    .regex(/^[a-zA-Z0-9._-]+$/)
    .optional(),
  args: z.array(z.string().max(500)).max(20).optional(),
  env: z.record(z.string().regex(/^[A-Z_][A-Z0-9_]*$/), z.string().max(1000)).optional(),
  url: z.string().url().optional(),
  headers: z.record(z.string().max(200), z.string().max(1000)).optional(),
})

const McpConfigSchema = z.object({
  mcpServers: z
    .record(z.string().max(100), McpServerConfigSchema)
    .refine((obj) => Object.keys(obj).length <= 50, { message: 'Too many servers (max 50)' }),
})

type McpConfig = z.infer<typeof McpConfigSchema>

// ── Config path ───────────────────────────────────────────────────────────────

const MCP_JSON_PATH = process.env.AXON_DATA_DIR
  ? path.join(process.env.AXON_DATA_DIR, 'axon', 'mcp.json')
  : path.join(os.homedir(), '.config', 'axon', 'mcp.json')

async function readMcpConfig(): Promise<McpConfig> {
  try {
    const raw = await fs.readFile(MCP_JSON_PATH, 'utf8')
    const json = JSON.parse(raw) as unknown
    const result = McpConfigSchema.safeParse(json)
    if (!result.success) {
      logError('api.mcp.config_validation_failed', { error: result.error.flatten() })
      return { mcpServers: {} }
    }
    return result.data
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
      return { mcpServers: {} }
    }
    throw err
  }
}

async function writeMcpConfig(config: McpConfig): Promise<void> {
  const dir = path.dirname(MCP_JSON_PATH)
  await fs.mkdir(dir, { recursive: true })
  await fs.writeFile(MCP_JSON_PATH, JSON.stringify(config, null, 2), 'utf8')
}

export async function GET() {
  try {
    const config = await readMcpConfig()
    return NextResponse.json(config)
  } catch (err) {
    logError('api.mcp.get_failed', { message: err instanceof Error ? err.message : String(err) })
    return NextResponse.json({ error: 'Failed to read mcp.json' }, { status: 500 })
  }
}

export async function PUT(request: NextRequest) {
  try {
    const body = (await request.json()) as unknown
    const result = McpConfigSchema.safeParse(body)
    if (!result.success) {
      return NextResponse.json(
        {
          error: 'Body must have mcpServers: Record<string, McpServerConfig>',
          details: result.error.flatten(),
        },
        { status: 400 },
      )
    }
    // SSRF guard: validate any HTTP server URLs before persisting
    for (const [, serverCfg] of Object.entries(result.data.mcpServers)) {
      if (serverCfg.url !== undefined && !validateStatusUrl(serverCfg.url)) {
        return NextResponse.json(
          { error: 'Server URL is not allowed (SSRF protection)' },
          { status: 400 },
        )
      }
    }
    await writeMcpConfig(result.data)
    return NextResponse.json({ ok: true })
  } catch (err) {
    logError('api.mcp.put_failed', { message: err instanceof Error ? err.message : String(err) })
    return NextResponse.json({ error: 'Failed to write mcp.json' }, { status: 500 })
  }
}

export async function DELETE(request: NextRequest) {
  try {
    const body = (await request.json()) as unknown
    if (
      !body ||
      typeof body !== 'object' ||
      !('name' in body) ||
      typeof (body as { name: unknown }).name !== 'string'
    ) {
      return NextResponse.json({ error: 'Body must have name: string' }, { status: 400 })
    }
    const { name } = body as { name: string }
    const config = await readMcpConfig()
    const updated: McpConfig = {
      mcpServers: Object.fromEntries(Object.entries(config.mcpServers).filter(([k]) => k !== name)),
    }
    await writeMcpConfig(updated)
    return NextResponse.json({ ok: true })
  } catch (err) {
    logError('api.mcp.delete_failed', { message: err instanceof Error ? err.message : String(err) })
    return NextResponse.json({ error: 'Failed to delete server from mcp.json' }, { status: 500 })
  }
}
