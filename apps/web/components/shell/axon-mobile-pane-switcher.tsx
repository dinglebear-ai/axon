'use client'

import {
  Brain,
  MessageSquare,
  Network,
  PenLine,
  ScrollText,
  Settings2,
  TerminalSquare,
} from 'lucide-react'
import type { ComponentType } from 'react'
import { Button } from '@/components/ui/button'
import type { MobilePane } from '@/hooks/use-split-pane'

interface AxonMobilePaneSwitcherProps {
  mobilePane: MobilePane
  onMobilePaneChange: (pane: MobilePane) => void
}

const PANE_BUTTONS: {
  id: MobilePane
  Icon: ComponentType<{ className?: string }>
  label: string
  activeClass: string
}[] = [
  {
    id: 'chat',
    Icon: MessageSquare,
    label: 'Chat pane',
    activeClass:
      'border-[rgba(175,215,255,0.48)] bg-[linear-gradient(145deg,rgba(135,175,255,0.34),rgba(135,175,255,0.14))] text-[var(--text-primary)] shadow-[0_0_14px_rgba(135,175,255,0.2)]',
  },
  {
    id: 'editor',
    Icon: PenLine,
    label: 'Editor pane',
    activeClass:
      'border-[rgba(255,135,175,0.45)] bg-[linear-gradient(145deg,rgba(255,135,175,0.3),rgba(255,135,175,0.12))] text-[var(--text-primary)] shadow-[0_0_14px_rgba(255,135,175,0.16)]',
  },
  {
    id: 'terminal',
    Icon: TerminalSquare,
    label: 'Terminal pane',
    activeClass:
      'border-[rgba(175,215,255,0.48)] bg-[linear-gradient(145deg,rgba(135,175,255,0.34),rgba(135,175,255,0.14))] text-[var(--text-primary)] shadow-[0_0_14px_rgba(135,175,255,0.2)]',
  },
  {
    id: 'logs',
    Icon: ScrollText,
    label: 'Logs pane',
    activeClass:
      'border-[rgba(175,215,255,0.48)] bg-[linear-gradient(145deg,rgba(135,175,255,0.34),rgba(135,175,255,0.14))] text-[var(--text-primary)] shadow-[0_0_14px_rgba(135,175,255,0.2)]',
  },
  {
    id: 'mcp',
    Icon: Network,
    label: 'MCP pane',
    activeClass:
      'border-[rgba(175,215,255,0.48)] bg-[linear-gradient(145deg,rgba(135,175,255,0.34),rgba(135,175,255,0.14))] text-[var(--text-primary)] shadow-[0_0_14px_rgba(135,175,255,0.2)]',
  },
  {
    id: 'cortex',
    Icon: Brain,
    label: 'Cortex pane',
    activeClass:
      'border-[rgba(175,215,255,0.48)] bg-[linear-gradient(145deg,rgba(135,175,255,0.34),rgba(135,175,255,0.14))] text-[var(--text-primary)] shadow-[0_0_14px_rgba(135,175,255,0.2)]',
  },
  {
    id: 'settings',
    Icon: Settings2,
    label: 'Settings pane',
    activeClass:
      'border-[rgba(175,215,255,0.48)] bg-[linear-gradient(145deg,rgba(135,175,255,0.34),rgba(135,175,255,0.14))] text-[var(--text-primary)] shadow-[0_0_14px_rgba(135,175,255,0.2)]',
  },
]

const INACTIVE_CLASS =
  'border-[var(--border-subtle)] bg-[rgba(10,18,35,0.42)] text-[var(--text-dim)] hover:-translate-y-0.5 hover:border-[rgba(175,215,255,0.3)] hover:text-[var(--axon-primary-strong)]'

export function AxonMobilePaneSwitcher({
  mobilePane,
  onMobilePaneChange,
}: AxonMobilePaneSwitcherProps) {
  return (
    <div role="tablist" aria-label="Workspace pane" className="inline-flex items-center gap-0.5">
      {PANE_BUTTONS.map(({ id, Icon, label, activeClass }) => (
        <Button
          key={id}
          type="button"
          variant="ghost"
          size="icon-sm"
          role="tab"
          aria-selected={mobilePane === id}
          aria-label={label}
          onClick={() => onMobilePaneChange(id)}
          className={`inline-flex size-10 items-center justify-center rounded border transition-colors duration-200 backdrop-blur-sm ${
            mobilePane === id ? activeClass : INACTIVE_CLASS
          }`}
        >
          <Icon className="size-4" />
        </Button>
      ))}
    </div>
  )
}
