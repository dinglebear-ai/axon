'use client'

import '@xterm/xterm/css/xterm.css'
import type { ITerminalOptions } from '@xterm/xterm'
import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react'

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface TerminalHandle {
  write: (data: string) => void
  writeln: (data: string) => void
  clear: () => void
  focus: () => void
  search: (
    query: string,
    opts?: { caseSensitive?: boolean; regex?: boolean; wholeWord?: boolean },
  ) => boolean
  getSelectedText: () => string
  resize: () => void
}

export interface TerminalEmulatorProps {
  onData: (data: string) => void
  onResize?: (cols: number, rows: number) => void
  className?: string
}

// ---------------------------------------------------------------------------
// Terminal theme — axon dark neural-tech palette
// ---------------------------------------------------------------------------

const TERMINAL_OPTIONS: ITerminalOptions = {
  allowProposedApi: true,
  theme: {
    background: 'rgba(0,0,0,0)',
    foreground: '#e8f4f8',
    cursor: '#87afff',
    cursorAccent: '#030712',
    black: '#0a1222',
    brightBlack: '#3d5a7a',
    red: '#ff6b6b',
    brightRed: '#ff87af',
    green: '#82d9a0',
    brightGreen: '#9ef5b8',
    yellow: '#ffc086',
    brightYellow: '#ffd4a8',
    blue: '#87afff',
    brightBlue: '#afd7ff',
    magenta: '#ff87af',
    brightMagenta: '#ff9ec0',
    cyan: '#7ad4e6',
    brightCyan: '#a0e8f8',
    white: '#b8cfe0',
    brightWhite: '#e8f4f8',
    selectionBackground: 'rgba(135,175,255,0.3)',
    selectionForeground: '#e8f4f8',
  },
  fontFamily: '"Noto Sans Mono", "JetBrains Mono", "Fira Code", monospace',
  fontSize: 13,
  lineHeight: 1.4,
  letterSpacing: 0,
  cursorBlink: true,
  cursorStyle: 'bar',
  scrollback: 5000,
  smoothScrollDuration: 100,
  allowTransparency: true,
  convertEol: true,
  rightClickSelectsWord: true,
  overviewRuler: { width: 8 },
}

// ---------------------------------------------------------------------------
// Scrollbar style — injected once into document head
// ---------------------------------------------------------------------------

const SCROLLBAR_STYLE_ID = 'axon-terminal-scrollbar'

function injectScrollbarStyle(): void {
  if (typeof document === 'undefined') return
  if (document.getElementById(SCROLLBAR_STYLE_ID)) return

  const style = document.createElement('style')
  style.id = SCROLLBAR_STYLE_ID
  style.textContent = `
    .xterm,
    .xterm-viewport,
    .xterm-screen,
    .xterm-screen canvas {
      background: transparent !important;
    }
    .xterm-viewport::-webkit-scrollbar {
      width: 2px;
    }
    .xterm-viewport::-webkit-scrollbar-track {
      background: transparent;
    }
    .xterm-viewport::-webkit-scrollbar-thumb {
      background: rgba(135,175,255,0.2);
      border-radius: 1px;
    }
    .xterm-viewport::-webkit-scrollbar-thumb:hover {
      background: rgba(135,175,255,0.4);
    }
  `
  document.head.appendChild(style)
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/**
 * Core xterm.js terminal component. Browser-only — all xterm imports happen
 * inside useEffect via dynamic `await import()` so this module is safe to
 * import on the server; the actual terminal instantiation only runs client-side.
 *
 * Export as `TerminalEmulator` (default) and `TerminalEmulatorInner` (named)
 * so the SSR wrapper can pick it up via dynamic import.
 */
export const TerminalEmulator = forwardRef<TerminalHandle, TerminalEmulatorProps>(
  function TerminalEmulator({ onData, onResize, className }, ref) {
    const containerRef = useRef<HTMLDivElement>(null)

    // Always-current ref so the stable onData wrapper registered with xterm
    // calls the latest prop without needing to re-mount the terminal.
    const onDataRef = useRef(onData)
    onDataRef.current = onData

    // These refs hold the live xterm instances so imperative handle methods
    // have stable access without capturing stale closures.
    const termRef = useRef<import('@xterm/xterm').Terminal | null>(null)
    const fitAddonRef = useRef<import('@xterm/addon-fit').FitAddon | null>(null)
    const searchAddonRef = useRef<import('@xterm/addon-search').SearchAddon | null>(null)

    // Tracks whether the component is still mounted so debounced callbacks
    // and rAF-scheduled work can bail out after disposal.
    const mountedRef = useRef(true)

    // Holds the pending rAF handle for the ResizeObserver-triggered fit() call
    // so it can be cancelled in the cleanup path.
    const rafRef = useRef<number>(0)

    // Expose imperative handle to parent
    useImperativeHandle(ref, () => ({
      write(data: string) {
        termRef.current?.write(data)
      },
      writeln(data: string) {
        termRef.current?.writeln(data)
      },
      clear() {
        termRef.current?.clear()
      },
      focus() {
        termRef.current?.focus()
      },
      search(query, opts): boolean {
        if (!searchAddonRef.current) return false
        return searchAddonRef.current.findNext(query, {
          caseSensitive: opts?.caseSensitive ?? false,
          regex: opts?.regex ?? false,
          wholeWord: opts?.wholeWord ?? false,
          decorations: {
            matchBackground: 'rgba(255,192,134,0.3)',
            matchBorder: 'rgba(255,192,134,0.8)',
            matchOverviewRuler: '#ffc086',
            activeMatchBackground: 'rgba(135,175,255,0.5)',
            activeMatchBorder: 'rgba(135,175,255,1)',
            activeMatchColorOverviewRuler: '#87afff',
          },
        })
      },
      getSelectedText(): string {
        return termRef.current?.getSelection() ?? ''
      },
      resize() {
        fitAddonRef.current?.fit()
      },
    }))

    useEffect(() => {
      if (!containerRef.current) return

      let disposed = false
      let observer: ResizeObserver | null = null

      // Keep a local reference to the terminal and container so the cleanup
      // closure captures the right instance even if refs change.
      let terminal: import('@xterm/xterm').Terminal | null = null

      async function init() {
        const [{ Terminal }, { FitAddon }, { WebLinksAddon }, { SearchAddon }, { CanvasAddon }] =
          await Promise.all([
            import('@xterm/xterm'),
            import('@xterm/addon-fit'),
            import('@xterm/addon-web-links'),
            import('@xterm/addon-search'),
            import('@xterm/addon-canvas'),
          ])

        // Guard against unmount happening during the async import
        if (disposed || !containerRef.current) return

        injectScrollbarStyle()

        terminal = new Terminal(TERMINAL_OPTIONS)
        const fitAddon = new FitAddon()
        const webLinksAddon = new WebLinksAddon()
        const searchAddon = new SearchAddon()

        terminal.loadAddon(fitAddon)
        terminal.loadAddon(webLinksAddon)
        terminal.loadAddon(searchAddon)

        terminal.open(containerRef.current)

        // Canvas renderer — GPU-accelerated and supports allowTransparency,
        // unlike WebGL which overwrites the transparent background with a
        // solid clear-color. Significantly faster than the DOM renderer.
        try {
          terminal.loadAddon(new CanvasAddon())
        } catch {
          // Falls back to DOM renderer if canvas context is unavailable.
        }

        fitAddon.fit()

        // Expose instances via refs for imperative handle
        termRef.current = terminal
        fitAddonRef.current = fitAddon
        searchAddonRef.current = searchAddon

        // Forward keyboard input through a stable wrapper so the latest
        // onData prop is always called (avoids stale closure on re-renders).
        terminal.onData((data) => onDataRef.current(data))

        // Copy-on-select: debounced so mid-drag intermediate selections don't
        // spam clipboard.writeText (which is rate-limited on some browsers).
        let selectionTimer = 0
        terminal.onSelectionChange(() => {
          clearTimeout(selectionTimer)
          selectionTimer = window.setTimeout(() => {
            // Guard against running after component unmount/disposal.
            if (!mountedRef.current) return
            const sel = terminal!.getSelection()
            if (sel) navigator.clipboard?.writeText(sel).catch(() => {})
          }, 50)
        })

        // Visual bell via a brief opacity flash on the container
        terminal.onBell(() => {
          const el = containerRef.current
          if (!el) return
          el.style.transition = 'opacity 50ms'
          el.style.opacity = '0.5'
          setTimeout(() => {
            el.style.opacity = '1'
            setTimeout(() => (el.style.transition = ''), 100)
          }, 100)
        })

        // Ctrl+Shift+C → copy selection; Ctrl+Shift+V → paste from clipboard
        terminal.attachCustomKeyEventHandler((e: KeyboardEvent) => {
          if (e.ctrlKey && e.shiftKey && e.code === 'KeyC') {
            const sel = terminal!.getSelection()
            if (sel) navigator.clipboard?.writeText(sel).catch(() => {})
            return false
          }
          if (e.ctrlKey && e.shiftKey && e.code === 'KeyV' && e.type === 'keydown') {
            navigator.clipboard
              ?.readText()
              .then((text) => {
                if (text) onDataRef.current(text)
              })
              .catch(() => {})
            return false
          }
          return true
        })

        // Notify parent of resize events (fired by xterm after fit)
        if (onResize) {
          terminal.onResize(({ cols, rows }) => {
            onResize(cols, rows)
          })
        }

        // Refit whenever the container changes size. rAF-debounced so CSS
        // transitions (dialog open/close scale animation) don't flood fit()
        // with dozens of calls per frame. The handle is stored in rafRef so
        // it can be cancelled during component disposal.
        observer = new ResizeObserver(() => {
          cancelAnimationFrame(rafRef.current)
          rafRef.current = requestAnimationFrame(() => fitAddon.fit())
        })
        observer.observe(containerRef.current)
      }

      init()

      return () => {
        disposed = true
        mountedRef.current = false
        cancelAnimationFrame(rafRef.current)
        observer?.disconnect()
        observer = null

        // Dispose in the next microtask so any in-flight writes complete first
        const term = terminal ?? termRef.current
        if (term) {
          try {
            term.dispose()
          } catch {
            // xterm.js addons (e.g. WebLinksAddon) can throw during dispose
            // if their internal references are already torn down
          }
        }
        termRef.current = null
        fitAddonRef.current = null
        searchAddonRef.current = null
      }
      // onData and onResize are intentionally excluded — they are called
      // through the live closure without needing to re-mount the terminal.
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [onResize])

    return (
      <div ref={containerRef} className={className} style={{ width: '100%', height: '100%' }} />
    )
  },
)

// Named alias for dynamic import in the wrapper
export { TerminalEmulator as TerminalEmulatorInner }
