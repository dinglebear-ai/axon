'use client'

import { X } from 'lucide-react'
import { memo, useCallback, useEffect, useRef, useState } from 'react'
import type { TerminalHandle } from '@/components/terminal/terminal-emulator'
import { TerminalEmulatorWrapper } from '@/components/terminal/terminal-emulator-wrapper'
import { TerminalToolbar } from '@/components/terminal/terminal-toolbar'
import { Button } from '@/components/ui/button'
import { useShellSession } from '@/hooks/use-shell-session'

export const AxonTerminalPane = memo(function AxonTerminalPane() {
  const terminalRef = useRef<TerminalHandle | null>(null)
  const [searchVisible, setSearchVisible] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const { status, sendInput, resize } = useShellSession({
    onOutput: (data) => terminalRef.current?.write(data),
  })

  const handleData = useCallback(
    (data: string) => {
      sendInput(data)
    },
    [sendInput],
  )

  const handleResize = useCallback(
    (cols: number, rows: number) => {
      resize(cols, rows)
    },
    [resize],
  )

  useEffect(() => {
    const timer = setTimeout(() => terminalRef.current?.focus(), 200)
    return () => clearTimeout(timer)
  }, [])

  return (
    <div className="flex h-full min-h-0 flex-col">
      <TerminalToolbar
        status={status}
        isRunning={false}
        onClear={() => terminalRef.current?.clear()}
        onCopy={() => {
          const text = terminalRef.current?.getSelectedText() ?? ''
          if (text)
            navigator.clipboard.writeText(text).catch((err) => {
              console.warn('Clipboard write failed:', err)
            })
        }}
        onCancelCurrent={() => {}}
        searchVisible={searchVisible}
        onToggleSearch={() => setSearchVisible((prev) => !prev)}
      />

      <div className="relative flex-1 overflow-hidden">
        {searchVisible ? (
          <div className="absolute right-3 top-2 z-20 flex items-center gap-1 rounded-md border border-[rgba(175,215,255,0.2)] bg-[rgba(9,18,37,0.95)] px-2 py-1">
            <input
              type="text"
              value={searchQuery}
              onChange={(event) => {
                setSearchQuery(event.target.value)
                if (event.target.value) terminalRef.current?.search(event.target.value)
              }}
              placeholder="Search…"
              className="w-40 bg-transparent font-mono text-xs text-[var(--text-primary)] outline-none"
              aria-label="Terminal search"
            />
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              onClick={() => {
                setSearchVisible(false)
                setSearchQuery('')
                terminalRef.current?.focus()
              }}
              className="ml-1 text-[var(--text-muted)] hover:text-[var(--text-primary)]"
              aria-label="Close search"
            >
              <X className="size-3" />
            </Button>
          </div>
        ) : null}

        <TerminalEmulatorWrapper
          ref={terminalRef}
          onData={handleData}
          onResize={handleResize}
          className="axon-terminal-container h-full w-full"
        />
      </div>
    </div>
  )
})
