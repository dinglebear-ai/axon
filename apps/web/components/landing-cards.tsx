'use client'

import { ChevronDown, ChevronRight, FolderOpen, Network } from 'lucide-react'
import Link from 'next/link'
import type React from 'react'
import { useEffect, useState } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { apiFetch } from '@/lib/api-fetch'

// ---------------------------------------------------------------------------
// Shared card shell
// ---------------------------------------------------------------------------

function LandingCard({
  icon,
  title,
  href,
  children,
  storageKey,
}: {
  icon: React.ReactNode
  title: string
  href?: string
  children: React.ReactNode
  storageKey: string
}) {
  const [open, setOpen] = useState(true)

  useEffect(() => {
    try {
      if (window.localStorage.getItem(storageKey) === 'collapsed') setOpen(false)
    } catch {
      // Ignore storage errors.
    }
  }, [storageKey])

  function handleOpenChange(next: boolean) {
    setOpen(next)
    try {
      if (!next) {
        window.localStorage.setItem(storageKey, 'collapsed')
      } else {
        window.localStorage.removeItem(storageKey)
      }
    } catch {
      // Ignore storage errors.
    }
  }

  return (
    <Collapsible open={open} onOpenChange={handleOpenChange}>
      <Card
        className="gap-0 border-[var(--border-subtle)] bg-[rgba(4,8,20,0.45)] py-0 shadow-none"
        style={{ minHeight: open ? '180px' : undefined }}
      >
        <CardHeader
          className="gap-0 px-3 py-2"
          style={{
            borderBottom: open ? '1px solid var(--border-subtle)' : undefined,
          }}
        >
          <div className="flex items-center justify-between">
            <CollapsibleTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                className="h-auto gap-1.5 p-0 hover:bg-transparent hover:opacity-80"
              >
                <span className="text-[rgba(175,215,255,0.55)] [&>svg]:size-3.5">{icon}</span>
                <span className="text-[10px] font-semibold uppercase tracking-widest text-[rgba(175,215,255,0.4)]">
                  {title}
                </span>
                <ChevronDown
                  className={`size-3 text-[rgba(175,215,255,0.3)] transition-transform duration-200 ${!open ? '-rotate-90' : ''}`}
                />
              </Button>
            </CollapsibleTrigger>
            {href && open && (
              <Link
                href={href}
                className="flex items-center gap-0.5 text-[10px] text-[rgba(175,215,255,0.3)] transition-colors hover:text-[rgba(175,215,255,0.7)]"
              >
                View all
                <ChevronRight className="size-3" />
              </Link>
            )}
          </div>
        </CardHeader>

        <CollapsibleContent>
          <CardContent className="overflow-hidden p-2">{children}</CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  )
}

// ---------------------------------------------------------------------------
// Dim helper text
// ---------------------------------------------------------------------------

function Dim({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full items-center justify-center py-4 text-[11px] italic text-[var(--text-dim)]">
      {children}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Files card
// ---------------------------------------------------------------------------

interface FileEntry {
  name: string
  type: 'file' | 'directory'
  path: string
}

function FilesContent() {
  const [entries, setEntries] = useState<FileEntry[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    const controller = new AbortController()
    apiFetch('/api/workspace?action=list&path=', { signal: controller.signal })
      .then((r) => r.json())
      .then((d: { items?: FileEntry[] }) => setEntries(d.items?.slice(0, 5) ?? []))
      .catch(() => {
        if (!controller.signal.aborted) setEntries([])
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false)
      })
    return () => controller.abort()
  }, [])

  if (loading) return <Dim>Loading...</Dim>
  if (entries.length === 0) return <Dim>Workspace empty or unavailable</Dim>

  return (
    <div className="flex flex-col gap-0.5">
      {entries.map((e) => (
        <Link
          key={e.path}
          href="/workspace"
          className="flex items-center gap-1.5 rounded px-2 py-1.5 transition-colors hover:bg-[var(--surface-float)]"
        >
          <span className="text-[rgba(175,215,255,0.4)]">
            {e.type === 'directory' ? (
              <FolderOpen className="size-3 shrink-0" />
            ) : (
              <span className="inline-block size-3 shrink-0" />
            )}
          </span>
          <span className="truncate font-mono text-[11px] text-[rgba(200,220,245,0.7)]">
            {e.name}
          </span>
        </Link>
      ))}
    </div>
  )
}

// ---------------------------------------------------------------------------
// MCP card
// ---------------------------------------------------------------------------

interface McpServerEntry {
  name: string
  type: 'stdio' | 'http'
  status: 'online' | 'offline' | 'unknown'
}

function McpContent() {
  const [servers, setServers] = useState<McpServerEntry[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    const controller = new AbortController()
    Promise.all([
      apiFetch('/api/mcp', { signal: controller.signal }).then((r) => r.json()) as Promise<{
        mcpServers?: Record<string, { url?: string }>
      }>,
      apiFetch('/api/mcp/status', { signal: controller.signal }).then((r) => r.json()) as Promise<{
        servers?: Record<string, { status: 'online' | 'offline' | 'unknown' }>
      }>,
    ])
      .then(([cfg, stat]) => {
        const entries: McpServerEntry[] = Object.entries(cfg.mcpServers ?? {})
          .slice(0, 5)
          .map(([name, s]) => ({
            name,
            type: s.url ? 'http' : 'stdio',
            status: stat.servers?.[name]?.status ?? 'unknown',
          }))
        setServers(entries)
      })
      .catch(() => {
        if (!controller.signal.aborted) setServers([])
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false)
      })
    return () => controller.abort()
  }, [])

  if (loading) return <Dim>Loading...</Dim>
  if (servers.length === 0) return <Dim>No MCP servers configured</Dim>

  return (
    <div className="flex flex-col gap-0.5">
      {servers.map((s) => (
        <Link
          key={s.name}
          href="/settings/mcp"
          className="flex items-center justify-between rounded px-2 py-1.5 transition-colors hover:bg-[var(--surface-float)]"
        >
          <span className="flex min-w-0 items-center gap-1.5">
            <span
              className="size-1.5 shrink-0 rounded-full"
              style={{
                background:
                  s.status === 'online'
                    ? 'var(--axon-success)'
                    : s.status === 'offline'
                      ? 'var(--axon-secondary)'
                      : 'rgba(180,180,180,0.35)',
              }}
            />
            <span className="truncate text-[11px] text-[rgba(200,220,245,0.7)]">{s.name}</span>
          </span>
          <Badge
            variant="outline"
            className={`ml-2 shrink-0 rounded-full border-transparent px-1.5 text-[9px] font-semibold uppercase tracking-wider ${
              s.type === 'http'
                ? 'bg-[rgba(175,215,255,0.08)] text-[rgba(175,215,255,0.55)]'
                : 'bg-[rgba(255,135,175,0.08)] text-[rgba(255,135,175,0.55)]'
            }`}
          >
            {s.type}
          </Badge>
        </Link>
      ))}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

export function LandingCards() {
  return (
    <div className="mt-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
      <LandingCard
        icon={<FolderOpen />}
        title="Files"
        href="/workspace"
        storageKey="axon.landing.card.files"
      >
        <FilesContent />
      </LandingCard>
      <LandingCard
        icon={<Network />}
        title="MCP"
        href="/settings/mcp"
        storageKey="axon.landing.card.mcp"
      >
        <McpContent />
      </LandingCard>
    </div>
  )
}
