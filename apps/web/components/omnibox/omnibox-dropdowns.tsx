'use client'

import { Button } from '@/components/ui/button'
import type { CommandOptionValues } from '@/lib/command-options'
import type { LocalDocFile, MentionKind } from '@/lib/omnibox'
import type { ModeCategory, ModeDefinition, ModeId } from '@/lib/ws-protocol'
import { MODE_CATEGORY_LABELS, MODE_CATEGORY_ORDER } from '@/lib/ws-protocol'
import { CommandOptionsPanel } from '../command-options-panel'

interface ModeDropdownProps {
  effectiveDropdownOpen: boolean
  mentionKind: MentionKind
  activeMentionQuery: string | undefined
  groupedModes: Map<ModeCategory, ModeDefinition[]>
  mode: ModeId
  selectMode: (id: ModeId) => void
}

export function ModeDropdown({
  effectiveDropdownOpen,
  mentionKind,
  activeMentionQuery,
  groupedModes,
  mode,
  selectMode,
}: ModeDropdownProps) {
  return (
    <div
      className={`absolute left-0 right-0 top-[calc(100%+6px)] z-50 max-h-[65vh] space-y-1 overflow-y-auto rounded-xl border border-[var(--border-standard)] p-2 shadow-[0_16px_48px_rgba(0,0,0,0.5),0_0_0_1px_rgba(255,135,175,0.08)] backdrop-blur-xl transition-all duration-200 ${
        effectiveDropdownOpen
          ? 'visible translate-y-0 opacity-100'
          : 'invisible -translate-y-1 opacity-0'
      }`}
      style={{ background: 'rgba(15, 23, 42, 0.95)' }}
    >
      {mentionKind === 'mode' && activeMentionQuery && (
        <div className="px-2.5 pb-1 pt-1 text-[length:var(--text-2xs)] text-[var(--text-dim)]">
          Showing results for{' '}
          <span className="text-[var(--axon-secondary)]">@{activeMentionQuery}</span>
        </div>
      )}
      {MODE_CATEGORY_ORDER.map((cat) => {
        const items = groupedModes.get(cat)
        if (!items || items.length === 0) return null
        const visibleItems =
          mentionKind === 'mode' && activeMentionQuery
            ? items.filter(
                (m) =>
                  m.id.includes(activeMentionQuery.toLowerCase()) ||
                  m.label.toLowerCase().includes(activeMentionQuery.toLowerCase()),
              )
            : items
        if (visibleItems.length === 0) return null
        return (
          <div key={cat}>
            <div className="px-2.5 pb-1 pt-1.5 text-[length:var(--text-2xs)] font-bold uppercase tracking-[0.15em] text-[var(--text-dim)]">
              {MODE_CATEGORY_LABELS[cat]}
            </div>
            <div className="grid grid-cols-[repeat(auto-fill,minmax(118px,1fr))] gap-0.5">
              {visibleItems.map((m, idx) => (
                <Button
                  key={m.id}
                  variant="ghost"
                  size="sm"
                  onClick={() => selectMode(m.id)}
                  className={`justify-start gap-2 text-xs font-medium animate-fade-in-up ${
                    m.id === mode
                      ? 'bg-[rgba(175,215,255,0.12)] text-[var(--axon-primary-strong)]'
                      : 'text-[var(--text-muted)] hover:bg-[var(--surface-float)] hover:text-[var(--axon-secondary)]'
                  }`}
                  style={{ animationDelay: `${idx * 35}ms`, animationFillMode: 'backwards' }}
                >
                  <svg
                    className="size-3.5 shrink-0"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth={2}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <path d={m.icon} />
                  </svg>
                  <span>{m.label}</span>
                </Button>
              ))}
            </div>
          </div>
        )
      })}
    </div>
  )
}

interface OptionsPopoverProps {
  hasOptions: boolean
  optionsOpen: boolean
  mode: ModeId
  optionValues: CommandOptionValues
  onOptionValuesChange: (values: CommandOptionValues) => void
}

export function OptionsPopover({
  hasOptions,
  optionsOpen,
  mode,
  optionValues,
  onOptionValuesChange,
}: OptionsPopoverProps) {
  if (!hasOptions) return null
  return (
    <div
      className={`absolute right-0 top-[calc(100%+6px)] z-50 w-[min(560px,92vw)] rounded-xl border border-[var(--border-standard)] p-2 shadow-[0_16px_48px_rgba(0,0,0,0.5),0_0_0_1px_rgba(255,135,175,0.08)] backdrop-blur-xl transition-all duration-200 ${
        optionsOpen ? 'visible translate-y-0 opacity-100' : 'invisible -translate-y-1 opacity-0'
      }`}
      style={{ background: 'rgba(15, 23, 42, 0.95)' }}
    >
      <CommandOptionsPanel mode={mode} values={optionValues} onChange={onOptionValuesChange} />
    </div>
  )
}

interface FileSuggestionsPanelProps {
  fileSuggestions: LocalDocFile[]
  mentionKind: MentionKind
  mentionSelectionIndex: number
  omniboxPhase: string
  setMentionSelectionIndex: (value: number) => void
  applyFileMentionCandidate: (candidate: LocalDocFile) => boolean
}

export function FileSuggestionsPanel({
  fileSuggestions,
  mentionKind,
  mentionSelectionIndex,
  omniboxPhase,
  setMentionSelectionIndex,
  applyFileMentionCandidate,
}: FileSuggestionsPanelProps) {
  if (fileSuggestions.length === 0 || mentionKind !== 'file') return null
  return (
    <div className="rounded-lg border border-[var(--border-subtle)] bg-[rgba(10,18,35,0.45)] px-2 py-1.5">
      <div className="ui-label mb-1 flex items-center justify-between">
        <span>File Context</span>
        <span className="text-[var(--text-dim)]">{omniboxPhase.replace('-', ' ')}</span>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {fileSuggestions.map((candidate, idx) => (
          <Button
            key={candidate.id}
            variant="outline"
            size="xs"
            onClick={() => {
              setMentionSelectionIndex(idx)
              void applyFileMentionCandidate(candidate)
            }}
            className={`font-semibold ${
              idx === mentionSelectionIndex
                ? 'border-[rgba(175,215,255,0.5)] bg-[rgba(175,215,255,0.18)] text-[var(--axon-primary)]'
                : 'border-[rgba(255,135,175,0.25)] bg-[rgba(255,135,175,0.08)] text-[var(--axon-secondary)] hover:bg-[rgba(255,135,175,0.14)]'
            }`}
          >
            @{candidate.label}
          </Button>
        ))}
      </div>
      <div className="ui-meta mt-1">Tab/Enter apply · up/down change</div>
    </div>
  )
}

interface AttachedContextPanelProps {
  fileContextMentions: Record<string, LocalDocFile>
  onRemoveMention: (label: string) => void
}

export function AttachedContextPanel({
  fileContextMentions,
  onRemoveMention,
}: AttachedContextPanelProps) {
  const entries = Object.entries(fileContextMentions)
  if (entries.length === 0) return null
  return (
    <div className="rounded-lg border border-[rgba(95,135,175,0.35)] bg-[rgba(30,41,59,0.35)] px-2 py-1.5">
      <div className="ui-label mb-1">Attached Context</div>
      <div className="flex flex-wrap gap-1.5">
        {entries.map(([label]) => (
          <Button
            key={label}
            variant="outline"
            size="xs"
            onClick={() => onRemoveMention(label)}
            className="border-[rgba(95,135,175,0.45)] bg-[rgba(95,135,175,0.12)] font-semibold text-[var(--axon-secondary)]"
          >
            @{label} x
          </Button>
        ))}
      </div>
    </div>
  )
}

interface ModeAppliedFeedbackProps {
  modeAppliedLabel: string | null
}

export function ModeAppliedFeedback({ modeAppliedLabel }: ModeAppliedFeedbackProps) {
  if (!modeAppliedLabel) return null
  return (
    <div className="rounded-md border border-[rgba(175,215,255,0.35)] bg-[rgba(175,215,255,0.1)] px-2 py-1 text-[length:var(--text-xs)] text-[var(--axon-primary)]">
      Mode selected:{' '}
      <span className="font-semibold text-[var(--axon-primary-strong)]">{modeAppliedLabel}</span>
    </div>
  )
}
