'use client'

import { MessageSquareText, PanelLeft, PanelRight } from 'lucide-react'
import { Button } from '@/components/ui/button'

const ICONS = {
  Sidebar: PanelLeft,
  Chat: MessageSquareText,
  Editor: PanelRight,
} as const

export function AxonPaneHandle({
  label,
  side,
  onClick,
}: {
  label: string
  side: 'left' | 'right'
  onClick: () => void
}) {
  const Icon = ICONS[label as keyof typeof ICONS] ?? (side === 'left' ? PanelLeft : PanelRight)
  return (
    <Button
      variant="ghost"
      size="sm"
      onClick={onClick}
      aria-label={`Expand ${label}`}
      className={`flex h-full w-9 flex-col items-center justify-start rounded-none bg-[linear-gradient(180deg,rgba(9,17,35,0.8),rgba(6,12,26,0.88))] pt-1.5 text-[var(--text-dim)] hover:bg-transparent hover:text-[var(--axon-primary)] ${side === 'left' ? 'border-r border-[var(--border-subtle)]' : 'border-l border-[var(--border-subtle)]'}`}
    >
      <span className="flex size-6 items-center justify-center rounded border border-transparent transition-colors hover:border-[rgba(175,215,255,0.2)] hover:bg-[rgba(175,215,255,0.08)]">
        <Icon className="size-3" />
      </span>
    </Button>
  )
}
