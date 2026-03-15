'use client'

import { Network, Plus, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { toast } from 'sonner'
import {
  configToForm,
  EMPTY_FORM,
  type FormState,
  type McpConfig,
  McpServerCard,
  type McpServerConfig,
  McpServerForm,
  type McpServerStatus,
} from '@/app/settings/mcp/components'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { ErrorBoundary } from '@/components/ui/error-boundary'
import { apiFetch } from '@/lib/api-fetch'
import { McpIcon } from './mcp-config'

function EmptyState({ onAdd }: { onAdd: () => void }) {
  return (
    <div className="flex min-h-[180px] flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-[var(--border-subtle)] bg-[var(--surface-float)] p-6 text-center">
      <Network className="size-8 text-[var(--axon-primary)]" />
      <div className="space-y-1">
        <p className="text-sm font-semibold text-[var(--text-primary)]">
          No MCP servers configured
        </p>
        <p className="text-xs text-[var(--text-muted)]">
          MCP servers extend Claude&apos;s capabilities with external tools, APIs, and data sources.
        </p>
      </div>
      <Button
        variant="outline"
        size="sm"
        onClick={onAdd}
        className="gap-1.5 border-[var(--border-standard)] bg-[rgba(135,175,255,0.15)] text-[12px] font-semibold text-[var(--axon-primary)] hover:bg-[rgba(135,175,255,0.25)]"
      >
        <Plus className="size-3.5" />
        Add your first server
      </Button>
    </div>
  )
}

function McpDialogContent() {
  const [config, setConfig] = useState<McpConfig>({ mcpServers: {} })
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [formOpen, setFormOpen] = useState(false)
  const [editTarget, setEditTarget] = useState<string | null>(null)
  const [deleteModal, setDeleteModal] = useState<string | null>(null)
  const [statusMap, setStatusMap] = useState<Record<string, McpServerStatus>>({})

  const loadStatus = useCallback(async (signal?: AbortSignal) => {
    try {
      const res = await apiFetch('/api/mcp/status', { signal })
      if (!res.ok) return
      const data = (await res.json()) as {
        servers: Record<string, { status: McpServerStatus; error?: string }>
      }
      setStatusMap(Object.fromEntries(Object.entries(data.servers).map(([k, v]) => [k, v.status])))
    } catch (err) {
      void err
    }
  }, [])

  const loadConfig = useCallback(
    async (signal?: AbortSignal) => {
      try {
        const res = await apiFetch('/api/mcp', { signal })
        if (!res.ok) throw new Error(`HTTP ${res.status}`)
        const data = (await res.json()) as McpConfig
        setConfig(data)
        setError('')
        setStatusMap(
          Object.fromEntries(
            Object.keys(data.mcpServers).map((k) => [k, 'checking' as McpServerStatus]),
          ),
        )
        void loadStatus(signal)
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') return
        setError(err instanceof Error ? err.message : 'Failed to load')
      } finally {
        setLoading(false)
      }
    },
    [loadStatus],
  )

  useEffect(() => {
    const controller = new AbortController()
    void loadConfig(controller.signal)
    return () => controller.abort()
  }, [loadConfig])

  async function saveServer(name: string, cfg: McpServerConfig) {
    const previousConfig: McpConfig = config
    const mergedConfig: McpConfig = { mcpServers: { ...config.mcpServers, [name]: cfg } }
    setConfig(mergedConfig)
    setFormOpen(false)
    setEditTarget(null)
    try {
      const res = await apiFetch('/api/mcp', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json', 'X-Pulse-Request': '1' },
        body: JSON.stringify(mergedConfig),
      })
      if (!res.ok) {
        setConfig(previousConfig)
        setError('Save failed')
        toast.error('Failed to save server configuration')
      } else {
        toast.success(`Server "${name}" saved`)
      }
    } catch (_err) {
      setConfig(previousConfig)
      setError('Network failure: Save aborted')
      toast.error('Network failure: Save aborted')
    }
  }

  async function deleteServer(name: string) {
    const res = await apiFetch('/api/mcp', {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json', 'X-Pulse-Request': '1' },
      body: JSON.stringify({ name }),
    })
    if (!res.ok) {
      setError('Delete failed')
      toast.error(`Failed to delete "${name}"`)
      return
    }
    toast.success(`Server "${name}" deleted`)
    const controller = new AbortController()
    await loadConfig(controller.signal)
  }

  const servers = Object.entries(config.mcpServers)
  const existingNames = servers.map(([n]) => n)
  const formInitial: FormState =
    editTarget && config.mcpServers[editTarget]
      ? configToForm(editTarget, config.mcpServers[editTarget])
      : EMPTY_FORM

  function openAdd() {
    setEditTarget(null)
    setFormOpen(true)
  }

  return (
    <div
      className="relative flex flex-col overflow-hidden"
      style={{ maxHeight: 'calc(85dvh - 3.5rem)' }}
    >
      <div className="flex shrink-0 items-center justify-between border-b border-[var(--border-subtle)] px-3 py-2">
        {error ? <span className="text-xs text-red-400">{error}</span> : <span />}
        <Button
          variant="outline"
          size="sm"
          onClick={openAdd}
          className="gap-1.5 border-[rgba(175,215,255,0.18)] bg-[rgba(175,215,255,0.07)] text-[12px] font-semibold text-[var(--axon-primary-strong)] hover:bg-[rgba(175,215,255,0.13)]"
        >
          <Plus className="size-3.5" />
          Add Server
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        {formOpen && (
          <div className="mb-4">
            <McpServerForm
              key={editTarget ?? '__new__'}
              initial={formInitial}
              existingNames={existingNames}
              isEditing={editTarget !== null}
              onSave={saveServer}
              onCancel={() => {
                setFormOpen(false)
                setEditTarget(null)
              }}
            />
          </div>
        )}

        {loading ? (
          <div className="flex items-center justify-center py-20">
            <div className="size-6 animate-spin rounded-full border-2 border-[rgba(175,215,255,0.2)] border-t-[var(--axon-primary-strong)]" />
          </div>
        ) : servers.length === 0 && !formOpen ? (
          <EmptyState onAdd={openAdd} />
        ) : (
          <div className="space-y-2">
            {servers.map(([name, cfg]) => (
              <McpServerCard
                key={name}
                name={name}
                cfg={cfg}
                status={statusMap[name] ?? 'unknown'}
                onEdit={() => {
                  setEditTarget(name)
                  setFormOpen(true)
                }}
                onDelete={() => setDeleteModal(name)}
              />
            ))}
          </div>
        )}
      </div>

      <AlertDialog
        open={deleteModal !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteModal(null)
        }}
      >
        <AlertDialogContent className="border-[var(--border-standard)] bg-[var(--surface-base)]">
          <AlertDialogHeader>
            <AlertDialogTitle className="flex items-center gap-2 text-sm text-[var(--text-primary)]">
              <Trash2 className="size-4 text-[var(--axon-secondary)]" />
              Delete &ldquo;{deleteModal}&rdquo;?
            </AlertDialogTitle>
            <AlertDialogDescription className="text-xs text-[var(--text-muted)]">
              This MCP server configuration will be permanently removed.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel className="text-xs">Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              className="text-xs"
              onClick={() => {
                if (deleteModal) {
                  void deleteServer(deleteModal)
                }
                setDeleteModal(null)
              }}
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

export function AxonMcpDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex max-h-[85dvh] w-full max-w-2xl flex-col gap-0 overflow-hidden border-[var(--border-subtle)] bg-[var(--glass-overlay)] p-0 text-[var(--text-primary)] backdrop-blur-xl sm:max-w-2xl"
        showCloseButton
      >
        <DialogHeader className="shrink-0 border-b border-[var(--border-subtle)] px-3 py-2.5">
          <DialogTitle className="flex items-center gap-2 text-[13px] font-semibold text-[var(--text-primary)]">
            <McpIcon className="size-4 text-[var(--axon-primary-strong)]" />
            MCP Servers
          </DialogTitle>
          <DialogDescription className="sr-only">
            Manage MCP server configurations
          </DialogDescription>
        </DialogHeader>
        <ErrorBoundary>
          <McpDialogContent />
        </ErrorBoundary>
      </DialogContent>
    </Dialog>
  )
}
