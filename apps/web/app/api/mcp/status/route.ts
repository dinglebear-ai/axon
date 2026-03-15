import { execFile } from 'node:child_process'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { promisify } from 'node:util'
import { NextResponse } from 'next/server'
import { logError } from '@/lib/server/logger'

const execFileAsync = promisify(execFile)

type McpServerConfig = {
  command?: string
  args?: string[]
  env?: Record<string, string>
  url?: string
  headers?: Record<string, string>
}

type McpConfig = {
  mcpServers: Record<string, McpServerConfig>
}

type ServerStatus = 'online' | 'offline' | 'unknown' | 'auth-required'

const MCP_JSON_PATH = process.env.AXON_DATA_DIR
  ? path.join(process.env.AXON_DATA_DIR, 'axon', 'mcp.json')
  : path.join(os.homedir(), '.config', 'axon', 'mcp.json')

const BLOCKED_HOSTNAMES = new Set(['localhost', '127.0.0.1', '0.0.0.0', '::1'])
const PRIVATE_IP_PATTERNS = [
  /^127\./,
  /^10\./,
  /^172\.(1[6-9]|2[0-9]|3[01])\./,
  /^192\.168\./,
  /^169\.254\./,
  /^fc[0-9a-f]{2}:/i,
  /^fd[0-9a-f]{2}:/i,
  // IPv4-mapped IPv6 in dotted-decimal form: ::ffff:127.x, ::ffff:10.x, etc.
  /^::ffff:(127\.|10\.|172\.(1[6-9]|2[0-9]|3[01])\.|192\.168\.|169\.254\.)/i,
  // IPv4-mapped IPv6 in hex-group form (e.g. ::ffff:7f00:1)
  /^::ffff:[0-9a-f]{1,4}:[0-9a-f]{1,4}$/i,
  // Link-local IPv6
  /^fe80:/i,
]

export function validateStatusUrl(url: string): boolean {
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return false
  }
  if (!['http:', 'https:'].includes(parsed.protocol)) return false
  // parsed.hostname includes brackets for IPv6 addresses (e.g. "[::1]") — strip them.
  // Also normalise trailing dots (e.g. "localhost." → "localhost").
  const hostname = parsed.hostname.replace(/^\[|\]$/g, '').replace(/\.+$/, '')
  if (BLOCKED_HOSTNAMES.has(hostname)) return false
  if (PRIVATE_IP_PATTERNS.some((p) => p.test(hostname))) return false
  return true
}

async function readMcpConfig(): Promise<McpConfig> {
  try {
    const raw = await fs.readFile(MCP_JSON_PATH, 'utf8')
    const parsed = JSON.parse(raw) as McpConfig
    if (!parsed.mcpServers || typeof parsed.mcpServers !== 'object') {
      return { mcpServers: {} }
    }
    return parsed
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') return { mcpServers: {} }
    throw err
  }
}

// MCP JSON-RPC ping payload — smallest valid request, returns immediately.
const MCP_PING = JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'ping' })

async function checkHttpServer(
  name: string,
  url: string,
  headers?: Record<string, string>,
): Promise<ServerStatus> {
  // POST a JSON-RPC ping with any configured auth headers.
  // GET opens an SSE stream that hangs; HEAD is not supported by most MCP servers.
  const reqHeaders: Record<string, string> = {
    'Content-Type': 'application/json',
    Accept: 'application/json, text/event-stream',
    ...headers,
  }
  try {
    const signal = AbortSignal.timeout(5_000)
    const res = await fetch(url, {
      method: 'POST',
      headers: reqHeaders,
      body: MCP_PING,
      signal,
      redirect: 'manual',
    })
    if (res.status === 401 || res.status === 403) return 'auth-required'
    // 3xx with manual redirect = OAuth gate or login redirect
    if (res.status >= 300 && res.status < 400) return 'auth-required'
    return res.status < 600 ? 'online' : 'offline'
  } catch (err) {
    logError('api.mcp.status.http_check_failed', {
      name,
      message: err instanceof Error ? err.message : String(err),
    })
    return 'offline'
  }
}

async function checkStdioServer(name: string, command: string): Promise<ServerStatus> {
  if (!command.trim()) return 'unknown'
  // Reject Windows-style paths — this server runs on Linux/macOS
  if (command.includes('\\')) return 'offline'
  try {
    if (path.isAbsolute(command)) {
      // Absolute path (e.g. /usr/local/bin/mcp-server): guard against .. *components*
      // only (not .. as part of a filename), then check existence.
      if (command.split('/').some((seg) => seg === '..')) return 'offline'
      await fs.access(command)
      return 'online'
    }
    // Relative command (e.g. python3, uvx, npx): must be a bare name — no separators,
    // no traversal sequences, no relative-path prefixes.
    if (command.includes('/') || command.includes('..') || command.startsWith('.')) {
      return 'offline'
    }
    await execFileAsync('which', [command], { timeout: 3_000 })
    return 'online'
  } catch (err) {
    logError('api.mcp.status.stdio_check_failed', {
      name,
      message: err instanceof Error ? err.message : String(err),
    })
    return 'offline'
  }
}

export async function GET() {
  try {
    const config = await readMcpConfig()
    const entries = Object.entries(config.mcpServers)

    const checks = entries.map(
      async ([name, cfg]): Promise<[string, { status: ServerStatus; error?: string }]> => {
        if (cfg.url) {
          if (!validateStatusUrl(cfg.url)) {
            return [name, { status: 'offline', error: 'invalid_url' }]
          }
          const status = await checkHttpServer(name, cfg.url, cfg.headers)
          return [name, { status }]
        }
        if (cfg.command) {
          const status = await checkStdioServer(name, cfg.command)
          return [name, { status }]
        }
        return [name, { status: 'unknown' }]
      },
    )

    const results = await Promise.all(checks)
    const servers = Object.fromEntries(results)

    return NextResponse.json({ servers })
  } catch (err) {
    logError('api.mcp.status.get_failed', {
      message: err instanceof Error ? err.message : String(err),
    })
    return NextResponse.json({ error: 'Failed to check MCP server status' }, { status: 500 })
  }
}
