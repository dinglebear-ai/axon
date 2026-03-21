import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { z } from 'zod'
import { logError } from '@/lib/server/logger'

// ── Schemas ─────────────────────────────────────────────────────────────────

export const McpServerConfigSchema = z.object({
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

export const McpConfigSchema = z.object({
  mcpServers: z
    .record(z.string().max(100), McpServerConfigSchema)
    .refine((obj) => Object.keys(obj).length <= 50, { message: 'Too many servers (max 50)' }),
})

export type McpConfig = z.infer<typeof McpConfigSchema>

// ── Config path ─────────────────────────────────────────────────────────────

export const MCP_JSON_PATH = process.env.AXON_DATA_DIR
  ? path.join(process.env.AXON_DATA_DIR, 'axon', 'mcp.json')
  : path.join(os.homedir(), '.config', 'axon', 'mcp.json')

// ── Read / Write ────────────────────────────────────────────────────────────

export async function readMcpConfig(): Promise<McpConfig> {
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

export async function writeMcpConfig(config: McpConfig): Promise<void> {
  const dir = path.dirname(MCP_JSON_PATH)
  await fs.mkdir(dir, { recursive: true })
  await fs.writeFile(MCP_JSON_PATH, JSON.stringify(config, null, 2), {
    encoding: 'utf8',
    mode: 0o600,
  })
  await fs.chmod(MCP_JSON_PATH, 0o600)
}
