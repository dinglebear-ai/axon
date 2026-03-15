'use client'

import {
  CheckCircle2,
  ChevronRight,
  SendHorizontal,
  Settings2,
  Shield,
  ShieldCheck,
  ShieldOff,
  SlidersHorizontal,
  Square,
  Wrench,
  XCircle,
} from 'lucide-react'
import Link from 'next/link'
import type React from 'react'
import { useEffect, useMemo } from 'react'
import { Button } from '@/components/ui/button'
import type {
  PulseWorkspaceAgent,
  PulseWorkspaceModel,
  PulseWorkspacePermission,
} from '@/hooks/ws-messages/types'
import type { CompletionStatus } from '@/lib/omnibox-types'
import { getAcpModelConfigOption } from '@/lib/pulse/acp-config'
import type { AcpConfigOption } from '@/lib/pulse/types'
import type { ModeDefinition, ModeId } from '@/lib/ws-protocol'
import { NO_INPUT_MODES } from '@/lib/ws-protocol'
import { PLACEHOLDER_TEXTS } from './utils'

const MENTION_TIP_SEEN_KEY = 'axon.web.mention-tip-seen'

interface OmniboxInputBarProps {
  input: string
  isProcessing: boolean
  statusText: string
  statusType: 'processing' | 'done' | 'error'
  completionStatus: CompletionStatus | null
  mode: ModeId
  selectedModeDef: ModeDefinition
  willRunAsCommand: boolean
  showModeSelector: boolean
  hasOptions: boolean
  activeOptionCount: number
  optionsOpen: boolean
  toolsOpen: boolean
  effectiveDropdownOpen: boolean
  modeAppliedLabel: string | null
  placeholderIdx: number
  placeholderVisible: boolean
  isFocused: boolean
  mentionTipSeen: boolean
  contextUtilizationPercent: number

  // Workspace
  workspaceMode: string | null
  workspaceContext: { contextBudgetChars: number; contextCharsTotal: number; turns: number } | null
  workspaceResumeSessionId: string | null
  pulseAgent: PulseWorkspaceAgent
  pulseModel: PulseWorkspaceModel
  pulsePermissionLevel: PulseWorkspacePermission
  acpConfigOptions: AcpConfigOption[]
  currentMode: string | null
  isProcessingWithCurrentMode: boolean

  // Refs
  inputRef: React.RefObject<HTMLTextAreaElement | null>
  toolsRef: React.RefObject<HTMLDivElement | null>

  // Actions
  setInput: (value: string) => void
  setDropdownOpen: (value: boolean | ((prev: boolean) => boolean)) => void
  setOptionsOpen: (value: boolean | ((prev: boolean) => boolean)) => void
  setToolsOpen: (value: boolean | ((prev: boolean) => boolean)) => void
  setIsFocused: (value: boolean) => void
  setMentionTipSeen: (value: boolean) => void
  setPulseAgent: (value: PulseWorkspaceAgent) => void
  setPulseModel: (value: PulseWorkspaceModel) => void
  setPulsePermissionLevel: (value: PulseWorkspacePermission) => void
  execute: () => void
  cancel: () => void
  handleKeyDown: (e: React.KeyboardEvent) => void
}

export function OmniboxInputBar(props: OmniboxInputBarProps) {
  const {
    input,
    isProcessing,
    statusText,
    statusType,
    completionStatus,
    mode,
    selectedModeDef,
    willRunAsCommand,
    showModeSelector,
    hasOptions,
    activeOptionCount,
    optionsOpen,
    toolsOpen,
    effectiveDropdownOpen,
    modeAppliedLabel,
    placeholderIdx,
    placeholderVisible: _placeholderVisible,
    isFocused,
    mentionTipSeen,
    contextUtilizationPercent,
    workspaceMode,
    workspaceContext,
    workspaceResumeSessionId,
    pulseAgent,
    pulseModel,
    pulsePermissionLevel,
    acpConfigOptions,
    currentMode: _currentMode,
    isProcessingWithCurrentMode,
    inputRef,
    toolsRef,
    setInput,
    setDropdownOpen,
    setOptionsOpen,
    setToolsOpen,
    setIsFocused,
    setMentionTipSeen,
    setPulseAgent,
    setPulseModel,
    setPulsePermissionLevel,
    execute,
    cancel,
    handleKeyDown,
  } = props

  const modelOptions = useMemo(() => {
    const modelConfig = getAcpModelConfigOption(acpConfigOptions)
    if (modelConfig) {
      return modelConfig.options.map((option) => ({ value: option.value, label: option.name }))
    }
    return [{ value: 'default', label: 'Default' }]
  }, [acpConfigOptions])

  useEffect(() => {
    const modelConfig = getAcpModelConfigOption(acpConfigOptions)
    if (!modelConfig || modelConfig.options.length === 0) {
      if (pulseModel !== 'default') {
        setPulseModel('default')
      }
      return
    }
    const hasCurrent = modelConfig.options.some((option) => option.value === pulseModel)
    if (!hasCurrent) {
      setPulseModel(modelConfig.currentValue || modelConfig.options[0]!.value)
    }
  }, [acpConfigOptions, pulseModel, setPulseModel])

  return (
    <div
      className={`relative flex min-h-[36px] items-center rounded-2xl transition-all duration-300 sm:min-h-[44px] ${
        isProcessing
          ? 'border-[rgba(175,215,255,0.4)] shadow-[0_0_20px_rgba(175,215,255,0.15)]'
          : 'border-[var(--border-accent)]'
      } focus-within:border-[var(--axon-secondary)] focus-within:shadow-[0_0_0_3px_var(--border-accent)]`}
      style={{
        background: 'rgba(10, 18, 35, 0.80)',
        borderWidth: '1.5px',
        borderStyle: 'solid',
        borderColor: isProcessing ? 'rgba(175,215,255, 0.4)' : 'var(--border-accent)',
      }}
    >
      {/* Processing sweep shimmer */}
      {isProcessing && (
        <div className="pointer-events-none absolute inset-0 overflow-hidden rounded-2xl">
          <div className="animate-omnibox-sweep absolute inset-0" />
        </div>
      )}
      {/* Bottom progress bar */}
      {isProcessing && (
        <div className="pointer-events-none absolute bottom-0 left-0 right-0 h-[2px] overflow-hidden rounded-b-2xl">
          <div className="animate-omnibox-progress h-full w-1/3" />
        </div>
      )}
      {/* Context utilization strip */}
      {!isProcessing && workspaceContext && workspaceContext.turns > 0 && (
        <div
          className="pointer-events-none absolute bottom-0 left-0 right-0 h-[2px] overflow-hidden rounded-b-2xl"
          title={`Context: ${contextUtilizationPercent.toFixed(1)}% · ${workspaceContext.contextCharsTotal.toLocaleString()} / ${workspaceContext.contextBudgetChars.toLocaleString()} chars`}
        >
          <div
            className="h-full bg-[linear-gradient(90deg,rgba(95,135,175,0.6),rgba(255,135,175,0.75))] transition-[width] duration-700"
            style={{
              width: `${contextUtilizationPercent}%`,
              minWidth: contextUtilizationPercent > 0 ? '3px' : undefined,
            }}
          />
        </div>
      )}
      {/* Text input */}
      <textarea
        id="axon-omnibox-input"
        name="axon_omnibox_input"
        ref={inputRef}
        value={input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={handleKeyDown}
        onFocus={() => setIsFocused(true)}
        onBlur={() => setIsFocused(false)}
        rows={1}
        placeholder={PLACEHOLDER_TEXTS[0]}
        className="min-w-0 flex-1 resize-none bg-transparent px-3 py-1.5 font-sans text-sm leading-[var(--leading-tight)] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-[var(--axon-primary)]/40 focus-visible:ring-offset-0 placeholder:opacity-0 sm:py-2 sm:px-4"
        style={{ overflowY: 'hidden' }}
        disabled={isProcessing}
      />
      {/* Animated placeholder overlay */}
      <span
        aria-hidden="true"
        className={`pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 select-none font-sans text-sm text-[var(--text-dim)] transition-opacity duration-300 sm:left-4 ${
          !input && !isProcessing ? 'opacity-100' : 'opacity-0'
        }`}
      >
        {PLACEHOLDER_TEXTS[placeholderIdx]}
      </span>
      {/* @mention discovery tip */}
      {!mentionTipSeen && isFocused && !input && (
        <div
          role="status"
          className="absolute -bottom-7 left-0 rounded border border-[var(--border-subtle)] bg-[var(--surface-base)] px-2 py-1 text-[10px] text-[var(--text-dim)] shadow-[var(--shadow-sm)] animate-fade-in backdrop-blur-sm"
          onMouseDown={(e) => e.preventDefault()}
        >
          Tip: type{' '}
          <kbd className="rounded border border-[var(--border-subtle)] px-1 font-mono text-[10px] text-[var(--text-muted)]">
            @
          </kbd>{' '}
          to attach a file
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={() => {
              setMentionTipSeen(true)
              localStorage.setItem(MENTION_TIP_SEEN_KEY, '1')
            }}
            className="ml-2 size-4 text-[var(--text-dim)] hover:text-[var(--text-muted)]"
            aria-label="Dismiss tip"
          >
            x
          </Button>
        </div>
      )}
      {/* Inline status */}
      <div
        className={`flex shrink-0 items-center gap-1.5 overflow-hidden whitespace-nowrap transition-all duration-300 ${
          statusText || completionStatus
            ? 'max-w-[280px] px-2 opacity-100'
            : 'max-w-0 px-0 opacity-0'
        }`}
      >
        {statusText ? (
          <>
            <span
              className={`size-1.5 shrink-0 rounded-full ${
                statusType === 'processing'
                  ? 'animate-pulse bg-[var(--axon-primary-strong)] shadow-[0_0_8px_rgba(175,215,255,0.7)]'
                  : statusType === 'done'
                    ? 'bg-[var(--axon-secondary)] shadow-[0_0_6px_rgba(255,135,175,0.5)]'
                    : 'bg-[var(--axon-error)] shadow-[0_0_6px_rgba(255,135,175,0.5)]'
              }`}
            />
            <span className="font-mono text-[length:var(--text-xs)] tracking-wide text-[var(--text-muted)]">
              {statusText}
            </span>
          </>
        ) : completionStatus ? (
          <div
            className={`flex items-center gap-1.5 text-xs transition-all duration-200 ${
              completionStatus.type === 'error'
                ? 'text-[var(--axon-secondary)]'
                : 'text-[var(--axon-success)]'
            }`}
          >
            {completionStatus.type === 'done' && <CheckCircle2 className="size-3" />}
            {completionStatus.type === 'error' && <XCircle className="size-3" />}
            <span className="font-mono text-[length:var(--text-xs)] tracking-wide">
              {completionStatus.text}
            </span>
          </div>
        ) : null}
      </div>
      {/* Divider */}
      <div className="h-[20px] w-px shrink-0 bg-[var(--border-subtle)]" />
      {/* Mode icon chip */}
      {showModeSelector && (
        <div className="inline-flex shrink-0 items-center px-2">
          <span
            className={`flex items-center justify-center rounded-full border p-1 transition-colors duration-200 ${
              willRunAsCommand && !NO_INPUT_MODES.has(mode) && input.trim().length > 0
                ? 'border-[rgba(175,215,255,0.38)] bg-[rgba(175,215,255,0.12)] text-[var(--axon-primary)]'
                : 'border-[rgba(95,135,175,0.28)] bg-[rgba(10,18,35,0.48)] text-[var(--axon-secondary)]'
            }`}
            title={selectedModeDef.label}
          >
            <svg
              className="size-3"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d={selectedModeDef.icon} />
            </svg>
          </span>
        </div>
      )}
      {/* Settings link */}
      {workspaceMode === 'pulse' && workspaceResumeSessionId && (
        <>
          <div className="h-[20px] w-px shrink-0 bg-[var(--border-subtle)]" />
          <span
            className="inline-flex items-center gap-1 rounded border border-[rgba(175,215,255,0.32)] bg-[rgba(175,215,255,0.12)] px-2 py-1 font-mono text-[10px] tracking-[0.04em] text-[var(--axon-primary)]"
            title={`Resuming session ${workspaceResumeSessionId}`}
          >
            RESUME
            <code className="text-[9px] text-[var(--text-muted)]">
              {workspaceResumeSessionId.slice(0, 8)}
            </code>
          </span>
        </>
      )}
      {/* Settings link */}
      <div className="h-[20px] w-px shrink-0 bg-[var(--border-subtle)]" />
      <Link
        href="/settings"
        className="flex items-center justify-center bg-transparent px-2 py-1.5 text-[var(--axon-secondary)] transition-colors duration-150 hover:text-white"
        title="Settings"
        aria-label="Open settings"
      >
        <Settings2 className="size-3.5" />
      </Link>
      {/* Pulse tools panel */}
      {workspaceMode === 'pulse' && (
        <>
          <div className="h-[20px] w-px shrink-0 bg-[var(--border-subtle)]" />
          <div ref={toolsRef} className="relative flex shrink-0 items-center">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setToolsOpen((prev: boolean) => !prev)
                setDropdownOpen(false)
                setOptionsOpen(false)
              }}
              className={`relative h-auto bg-transparent px-2 py-1.5 text-[var(--axon-secondary)] transition-colors duration-150 hover:bg-transparent hover:text-white ${
                toolsOpen ? 'text-white' : ''
              }`}
              title={`Pulse tools · ${pulseAgent} · ${pulseModel} · ${pulsePermissionLevel}`}
              aria-label="Pulse tools"
            >
              {pulsePermissionLevel === 'plan' ? (
                <Shield className="size-3.5" />
              ) : pulsePermissionLevel === 'bypass-permissions' ? (
                <ShieldOff className="size-3.5" />
              ) : (
                <ShieldCheck className="size-3.5" />
              )}
              {isProcessingWithCurrentMode && (
                <span className="pointer-events-none absolute -right-0.5 -top-0.5 inline-flex size-1.5 animate-pulse rounded-full bg-[var(--axon-primary-strong)]" />
              )}
            </Button>
            {toolsOpen && (
              <div
                className={[
                  'absolute bottom-[calc(100%+6px)] right-0 z-50 w-44 space-y-2 rounded-md',
                  'border border-[var(--border-standard)] bg-[rgba(10,18,35,0.96)]',
                  'p-2 shadow-[0_8px_24px_rgba(0,0,0,0.45)]',
                ].join(' ')}
                role="dialog"
                aria-label="Pulse model and permission controls"
              >
                <div className="flex items-center gap-1.5 text-[var(--text-dim)]">
                  <Wrench className="size-3" />
                  <span className="ui-label">Tools</span>
                </div>
                <label className="block space-y-1">
                  <span className="ui-label">Agent</span>
                  <select
                    id="omnibox-pulse-agent-selector"
                    name="omnibox_pulse_agent_selector"
                    value={pulseAgent}
                    onChange={(e) => setPulseAgent(e.target.value as PulseWorkspaceAgent)}
                    className="h-7 w-full rounded border border-[rgba(95,135,175,0.24)] bg-[rgba(10,18,35,0.72)] px-2 text-[length:var(--text-xs)] font-semibold uppercase tracking-[0.04em] text-[var(--text-primary)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--axon-primary)]/40"
                    aria-label="Agent selector"
                  >
                    <option value="claude">Claude</option>
                    <option value="codex">Codex</option>
                  </select>
                </label>
                <label className="block space-y-1">
                  <span className="ui-label">Model</span>
                  <select
                    id="omnibox-pulse-model-selector"
                    name="omnibox_pulse_model_selector"
                    value={pulseModel}
                    onChange={(e) => setPulseModel(e.target.value as PulseWorkspaceModel)}
                    className="h-7 w-full rounded border border-[rgba(95,135,175,0.24)] bg-[rgba(10,18,35,0.72)] px-2 text-[length:var(--text-xs)] font-semibold uppercase tracking-[0.04em] text-[var(--text-primary)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--axon-primary)]/40"
                    aria-label="Model selector"
                  >
                    {modelOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="block space-y-1">
                  <span className="ui-label">Permission</span>
                  <select
                    id="omnibox-pulse-permission-selector"
                    name="omnibox_pulse_permission_selector"
                    value={pulsePermissionLevel}
                    onChange={(e) =>
                      setPulsePermissionLevel(e.target.value as PulseWorkspacePermission)
                    }
                    className="h-7 w-full rounded border border-[var(--border-standard)] bg-[rgba(10,18,35,0.72)] px-2 text-[length:var(--text-xs)] font-semibold uppercase tracking-[0.04em] text-[var(--text-primary)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--axon-primary)]/40"
                    aria-label="Permission selector"
                  >
                    <option value="plan">Plan</option>
                    <option value="accept-edits">Accept</option>
                    <option value="bypass-permissions">Bypass</option>
                  </select>
                </label>
              </div>
            )}
          </div>
        </>
      )}
      {/* Options button */}
      {hasOptions && (
        <>
          <div className="h-[20px] w-px shrink-0 bg-[var(--border-subtle)]" />
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setOptionsOpen((prev: boolean) => !prev)
              setDropdownOpen(false)
            }}
            className={`relative h-auto shrink-0 bg-transparent px-2 py-1.5 transition-colors duration-150 hover:bg-transparent ${
              optionsOpen
                ? 'text-[var(--axon-primary)]'
                : 'text-[var(--text-muted)] hover:text-[var(--axon-secondary)]'
            }`}
            title="Command options"
          >
            <SlidersHorizontal className="size-3.5 shrink-0" />
            {activeOptionCount > 0 && (
              <span className="absolute -right-0.5 -top-0.5 inline-flex min-w-[14px] items-center justify-center rounded-full border border-[rgba(175,215,255,0.35)] bg-[rgba(175,215,255,0.12)] px-1 text-[length:var(--text-2xs)] leading-[var(--leading-tight)] text-[var(--axon-primary)]">
                {activeOptionCount}
              </span>
            )}
          </Button>
        </>
      )}
      {/* Divider before send/cancel */}
      <div className="h-[20px] w-px shrink-0 bg-[var(--border-subtle)]" />
      {/* Send / cancel */}
      <Button
        variant="ghost"
        size="sm"
        onClick={isProcessing ? cancel : execute}
        disabled={!isProcessing && !input.trim() && !NO_INPUT_MODES.has(mode)}
        className={`h-auto shrink-0 bg-transparent px-2.5 py-1.5 transition-all duration-200 hover:bg-transparent ${
          modeAppliedLabel
            ? 'text-[var(--axon-primary-strong)] drop-shadow-[0_0_8px_rgba(175,215,255,0.55)]'
            : input.trim().length > 0 && !isProcessing
              ? 'text-[var(--axon-secondary)] drop-shadow-[0_0_10px_rgba(255,135,175,0.5)] hover:text-white hover:drop-shadow-[0_0_14px_rgba(255,135,175,0.7)]'
              : 'text-[var(--axon-secondary)] hover:text-white'
        }`}
        title={isProcessing ? 'Cancel' : 'Execute'}
      >
        {isProcessing ? <Square className="size-3.5" /> : <SendHorizontal className="size-3.5" />}
      </Button>
      {/* Chevron — mode dropdown toggle */}
      {showModeSelector && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setDropdownOpen((prev: boolean) => !prev)}
          className="h-auto shrink-0 rounded-r-[10px] bg-transparent px-2 py-1.5 text-[var(--text-muted)] transition-colors duration-150 hover:bg-transparent hover:text-[var(--axon-secondary)]"
          title="Select mode"
        >
          <ChevronRight
            className={`size-3.5 transition-transform duration-200 ${effectiveDropdownOpen ? 'rotate-90' : ''}`}
          />
        </Button>
      )}
    </div>
  )
}
