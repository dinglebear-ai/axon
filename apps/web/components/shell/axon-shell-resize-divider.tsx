'use client'

export function AxonShellResizeDivider({
  onDragStart,
  onReset,
  onNudge,
}: {
  onDragStart: (startX: number) => void
  onReset?: () => void
  onNudge?: (delta: number) => void
}) {
  return (
    <button
      type="button"
      aria-label="Resize panel divider"
      title="Drag to resize · Double-click to reset · Arrow keys to nudge"
      className="group relative z-10 flex w-1.5 shrink-0 cursor-col-resize items-stretch focus-visible:outline-none"
      onMouseDown={(e) => {
        e.preventDefault()
        onDragStart(e.clientX)
      }}
      onDoubleClick={onReset}
      onKeyDown={(e) => {
        if (!onNudge) return
        if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return
        e.preventDefault()
        const step = e.shiftKey ? 50 : 10
        onNudge(e.key === 'ArrowRight' ? step : -step)
      }}
    >
      <div className="mx-auto h-full w-px bg-[var(--border-subtle)] transition-colors group-hover:bg-[rgba(175,215,255,0.3)] group-focus-visible:bg-[rgba(175,215,255,0.3)]" />
    </button>
  )
}
