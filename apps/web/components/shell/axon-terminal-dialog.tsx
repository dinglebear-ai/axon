'use client'

import { X } from 'lucide-react'
import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { AxonTerminalPane } from './axon-terminal-pane'

export function AxonTerminalDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  // Only mount the terminal after the first open — then keep it alive forever
  // so the shell session persists across open/close cycles.
  const [everOpened, setEverOpened] = useState(false)
  useEffect(() => {
    if (open) setEverOpened(true)
  }, [open])

  return (
    <>
      {/* Backdrop — only interactive/visible when open */}
      <div
        aria-hidden
        className={`fixed inset-0 z-50 bg-black/50 transition-opacity duration-200 ${
          open ? 'opacity-100' : 'pointer-events-none opacity-0'
        }`}
        onClick={() => onOpenChange(false)}
      />

      {/* Terminal panel — always mounted so the session persists */}
      <div
        role="dialog"
        aria-label="Terminal"
        aria-describedby="terminal-dialog-description"
        aria-modal={open}
        aria-hidden={!open}
        className={`fixed left-1/2 top-1/2 z-50 flex w-[min(92vw,960px)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-[18px] border border-[var(--border-subtle)] bg-[rgba(3,7,18,0.22)] shadow-[0_24px_64px_rgba(0,0,0,0.54)] backdrop-blur-2xl transition-all duration-200 ${
          open ? 'scale-100 opacity-100' : 'pointer-events-none scale-95 opacity-0'
        }`}
        style={{ height: 'min(72dvh, 640px)' }}
      >
        <p id="terminal-dialog-description" className="sr-only">
          Interactive shell terminal session
        </p>
        <div className="flex h-10 shrink-0 items-center justify-between border-b border-[rgba(175,215,255,0.06)] px-3">
          <span className="text-[11px] uppercase tracking-[0.16em] text-[var(--text-dim)]">
            Terminal
          </span>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => onOpenChange(false)}
            aria-label="Close terminal"
            className="size-6 text-[var(--text-dim)] hover:bg-[rgba(175,215,255,0.06)] hover:text-[var(--text-primary)]"
          >
            <X className="size-3.5" />
          </Button>
        </div>

        <div className="min-h-0 flex-1">{everOpened ? <AxonTerminalPane /> : null}</div>
      </div>
    </>
  )
}
