'use client'

import {
  FileCode2,
  Loader2,
  Paperclip,
  Shield,
  ShieldCheck,
  ShieldOff,
  Sparkles,
  Wrench,
  X,
} from 'lucide-react'
import React, { type ChangeEvent, useMemo, useRef, useState } from 'react'
import {
  PromptInput,
  PromptInputAttachments,
  PromptInputBody,
  PromptInputButton,
  type PromptInputFile,
  PromptInputFooter,
  type PromptInputMessage,
  PromptInputSubmit,
  PromptInputTextarea,
  PromptInputTools,
} from '@/components/ai-elements/prompt-input'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import type { McpServersState } from '@/hooks/use-mcp-servers'
import type { PulseAgent } from '@/lib/pulse/types'

type ToolPresetOption = { id: string; name: string }

const AGENT_OPTIONS: Array<{ value: PulseAgent; label: string }> = [
  { value: 'claude', label: 'Claude' },
  { value: 'codex', label: 'Codex' },
  { value: 'gemini', label: 'Gemini' },
]

const AXON_COMPOSER_PANEL_CLASS =
  'border-[rgba(175,215,255,0.2)] bg-[linear-gradient(180deg,rgba(10,18,35,0.94),rgba(4,9,20,0.98))] shadow-[0_14px_34px_rgba(0,0,0,0.3)] backdrop-blur-xl'

function ComposerDropdownTrigger({ children, ...props }: React.ComponentProps<'button'>) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      className="axon-icon-btn size-7"
      {...props}
    >
      {children}
    </Button>
  )
}

function getPermissionIcon(permissionLevel: string) {
  if (permissionLevel === 'plan') return Shield
  if (permissionLevel === 'bypass-permissions') return ShieldOff
  return ShieldCheck
}

function formatToolSelectionLabel(enabledCount: number, totalCount: number) {
  if (totalCount === 0) return 'No MCP tools'
  if (enabledCount === totalCount) return `Tools \u00b7 All ${totalCount}`
  if (enabledCount === 0) return 'Tools \u00b7 None'
  return `Tools \u00b7 ${enabledCount}/${totalCount}`
}

function AxonAttachmentPill({ file, onRemove }: { file: PromptInputFile; onRemove?: () => void }) {
  return (
    <span className="inline-flex max-w-full items-center gap-1.5 rounded border border-[rgba(175,215,255,0.16)] bg-[rgba(255,255,255,0.04)] px-2 py-1 text-xs leading-none text-[var(--text-secondary)]">
      <FileCode2 className="size-3.5 shrink-0 text-[var(--axon-primary)]" />
      <span className="truncate">{file.filename ?? file.url}</span>
      {onRemove ? (
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          onClick={onRemove}
          className="ml-0.5 size-4 rounded text-[var(--text-dim)] hover:text-[var(--text-primary)]"
          aria-label={`Remove ${file.filename ?? file.url}`}
        >
          <X className="size-3" />
        </Button>
      ) : null}
    </span>
  )
}

export const AxonPromptComposer = React.memo(function AxonPromptComposer({
  files,
  onFilesChange,
  onSubmit,
  modelOptions,
  permissionOptions,
  pulseModel,
  pulsePermissionLevel,
  onModelChange,
  onPermissionChange,
  toolsState,
  onToggleMcpServer,
  mcpToolsByServer,
  enabledMcpTools,
  onToggleMcpTool,
  onEnableServerTools,
  onDisableServerTools,
  toolPresets,
  onApplyToolPreset,
  onDeleteToolPreset,
  onSaveToolPreset,
  pulseAgent,
  onAgentChange,
  compact = false,
  isStreaming = false,
  connected = false,
}: {
  files: PromptInputFile[]
  onFilesChange: (files: PromptInputFile[]) => void
  onSubmit: (message: PromptInputMessage) => void | Promise<void>
  modelOptions: Array<{ value: string; label: string }>
  permissionOptions: Array<{ value: string; label: string }>
  pulseModel: string
  pulsePermissionLevel: string
  onModelChange: (value: string) => void
  onPermissionChange: (value: string) => void
  toolsState: McpServersState
  onToggleMcpServer: (serverName: string) => void
  mcpToolsByServer: Record<string, string[]>
  enabledMcpTools: string[]
  onToggleMcpTool: (toolName: string) => void
  onEnableServerTools: (serverName: string) => void
  onDisableServerTools: (serverName: string) => void
  toolPresets: ToolPresetOption[]
  onApplyToolPreset: (presetId: string) => void
  onDeleteToolPreset: (presetId: string) => void
  onSaveToolPreset: (name: string) => void
  pulseAgent: PulseAgent
  onAgentChange: (value: PulseAgent) => void
  compact?: boolean
  isStreaming?: boolean
  connected?: boolean
}) {
  const fileInputRef = useRef<HTMLInputElement | null>(null)
  const [presetDraft, setPresetDraft] = useState('')
  const PermissionIcon = getPermissionIcon(pulsePermissionLevel)
  const { toolLabel } = useMemo(() => {
    const tools = Object.values(mcpToolsByServer).flat()
    const serverSet = new Set(toolsState.enabledMcpServers)
    const toolSet = new Set(enabledMcpTools)
    const count = tools.filter((tool) => {
      const server = tool.split('__')[1] ?? ''
      return serverSet.has(server) && toolSet.has(tool)
    }).length
    const label = tools.length
      ? formatToolSelectionLabel(count, tools.length)
      : formatToolSelectionLabel(toolsState.enabledMcpServers.length, toolsState.mcpServers.length)
    return { toolLabel: label }
  }, [
    mcpToolsByServer,
    toolsState.enabledMcpServers,
    toolsState.mcpServers.length,
    enabledMcpTools,
  ])

  function handleFilePick(event: ChangeEvent<HTMLInputElement>) {
    const selectedFiles = Array.from(event.target.files ?? [])
    if (selectedFiles.length === 0) return
    onFilesChange([
      ...files,
      ...selectedFiles.map((file) => ({
        url: `file://${file.name}`,
        filename: file.name,
        mediaType: file.type || undefined,
      })),
    ])
    event.target.value = ''
  }

  return (
    <PromptInput
      onSubmit={onSubmit}
      files={files}
      onFilesChange={onFilesChange}
      className={`w-full rounded-[16px] p-0 sm:rounded-[14px] ${AXON_COMPOSER_PANEL_CLASS}`}
    >
      <input
        id="axon-prompt-file-input"
        name="axon_prompt_files"
        ref={fileInputRef}
        type="file"
        multiple
        className="hidden"
        onChange={handleFilePick}
      />
      <div className="px-2.5 pb-2.5 pt-2.5 sm:px-2 sm:pb-2 sm:pt-2">
        <PromptInputAttachments>
          {files.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {files.map((file, index) => (
                <AxonAttachmentPill
                  key={`${file.url}-${index}`}
                  file={file}
                  onRemove={() =>
                    onFilesChange(files.filter((_, fileIndex) => fileIndex !== index))
                  }
                />
              ))}
            </div>
          ) : null}
        </PromptInputAttachments>

        <PromptInputBody className="items-start gap-2">
          <PromptInputTextarea
            className={`axon-input ${compact ? 'min-h-10 max-h-40 sm:min-h-9' : 'min-h-14 max-h-56'} rounded-[10px] px-2.5 py-1.5 leading-5`}
            placeholder="Describe what you want to build, edit, or debug…"
          />
        </PromptInputBody>

        <PromptInputFooter className="mt-1.5 flex items-center justify-end gap-1.5">
          <div className="min-w-0 flex-1" />
          <div className="min-w-0 overflow-hidden">
            <PromptInputTools className="-mx-0.5 flex flex-nowrap items-center gap-2 overflow-x-auto px-0.5">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <ComposerDropdownTrigger aria-label={toolLabel}>
                    <Wrench className="size-3.5 text-[var(--axon-primary)]" />
                  </ComposerDropdownTrigger>
                </DropdownMenuTrigger>
                <DropdownMenuContent
                  align="end"
                  className="w-72 border-[var(--border-subtle)] bg-[var(--glass-overlay)] text-[var(--text-primary)] backdrop-blur-xl"
                >
                  <DropdownMenuLabel className="px-2 py-1 text-[11px] uppercase tracking-[0.14em] text-[var(--text-dim)]">
                    MCP server tools
                  </DropdownMenuLabel>
                  <div className="px-2 pb-2 text-[11px] leading-5 text-[var(--text-dim)]">
                    Server and per-tool toggles apply to the active chat immediately.
                  </div>
                  <DropdownMenuSeparator className="bg-[rgba(175,215,255,0.08)]" />
                  {toolsState.mcpServers.length > 0 ? (
                    toolsState.mcpServers.map((serverName) => {
                      const serverStatus = toolsState.mcpStatusByServer[serverName] ?? 'unknown'
                      const serverTools = mcpToolsByServer[serverName] ?? []
                      const serverEnabled = toolsState.enabledMcpServers.includes(serverName)
                      return (
                        <div key={serverName}>
                          <DropdownMenuCheckboxItem
                            checked={serverEnabled}
                            onCheckedChange={() => onToggleMcpServer(serverName)}
                            className="gap-2 py-2"
                          >
                            <span className="truncate">{serverName}</span>
                            <span
                              className={`ml-auto rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.12em] ${
                                serverStatus === 'online'
                                  ? 'bg-[rgba(64,196,128,0.12)] text-[rgba(128,220,160,0.92)]'
                                  : serverStatus === 'offline'
                                    ? 'bg-[rgba(255,135,175,0.12)] text-[rgba(255,170,196,0.86)]'
                                    : 'bg-[rgba(175,215,255,0.08)] text-[var(--text-dim)]'
                              }`}
                            >
                              {serverStatus}
                            </span>
                          </DropdownMenuCheckboxItem>
                          {serverTools.length > 0 ? (
                            <div className="pb-1 pl-6 pr-2">
                              <div className="mb-1 flex items-center gap-1">
                                <Button
                                  type="button"
                                  variant="outline"
                                  size="xs"
                                  className="h-auto px-1.5 py-0.5 text-[10px] text-[var(--text-dim)] hover:text-[var(--text-primary)]"
                                  onClick={(event) => {
                                    event.preventDefault()
                                    event.stopPropagation()
                                    onEnableServerTools(serverName)
                                  }}
                                >
                                  All on
                                </Button>
                                <Button
                                  type="button"
                                  variant="outline"
                                  size="xs"
                                  className="h-auto px-1.5 py-0.5 text-[10px] text-[var(--text-dim)] hover:text-[var(--text-primary)]"
                                  onClick={(event) => {
                                    event.preventDefault()
                                    event.stopPropagation()
                                    onDisableServerTools(serverName)
                                  }}
                                >
                                  All off
                                </Button>
                              </div>
                              {serverTools.map((toolName) => {
                                const shortName = toolName.split('__').slice(2).join('__')
                                return (
                                  <DropdownMenuCheckboxItem
                                    key={toolName}
                                    checked={enabledMcpTools.includes(toolName)}
                                    disabled={!serverEnabled}
                                    onCheckedChange={() => onToggleMcpTool(toolName)}
                                    className="py-1 text-xs"
                                  >
                                    <span className="truncate">{shortName}</span>
                                  </DropdownMenuCheckboxItem>
                                )
                              })}
                            </div>
                          ) : null}
                        </div>
                      )
                    })
                  ) : (
                    <div className="px-2 py-2 text-xs text-[var(--text-dim)]">
                      No MCP servers configured yet.
                    </div>
                  )}
                  <DropdownMenuSeparator className="bg-[rgba(175,215,255,0.08)]" />
                  <DropdownMenuLabel className="px-2 py-1 text-[11px] uppercase tracking-[0.14em] text-[var(--text-dim)]">
                    Tool presets
                  </DropdownMenuLabel>
                  <div className="space-y-2 px-2 pb-2">
                    <div className="flex items-center gap-1">
                      <input
                        id="axon-tool-preset-name"
                        name="axon_tool_preset_name"
                        value={presetDraft}
                        onChange={(event) => setPresetDraft(event.target.value)}
                        placeholder="Preset name"
                        className="h-7 min-w-0 flex-1 rounded border border-[var(--border-subtle)] bg-[rgba(255,255,255,0.03)] px-2 text-xs text-[var(--text-primary)] outline-none"
                      />
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        className="h-7 px-2 text-[11px] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
                        onClick={(event) => {
                          event.preventDefault()
                          event.stopPropagation()
                          const name = presetDraft.trim()
                          if (!name) return
                          onSaveToolPreset(name)
                          setPresetDraft('')
                        }}
                      >
                        Save
                      </Button>
                    </div>
                    {toolPresets.length > 0 ? (
                      <div className="max-h-36 space-y-1 overflow-y-auto">
                        {toolPresets.map((preset) => (
                          <div key={preset.id} className="flex items-center gap-1">
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              className="h-7 min-w-0 flex-1 justify-start truncate px-2 text-left text-xs text-[var(--text-primary)] hover:bg-[rgba(175,215,255,0.08)]"
                              onClick={(event) => {
                                event.preventDefault()
                                event.stopPropagation()
                                onApplyToolPreset(preset.id)
                              }}
                            >
                              {preset.name}
                            </Button>
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon-xs"
                              className="h-7 w-7 text-[11px] text-[var(--text-dim)] hover:text-[var(--text-primary)]"
                              onClick={(event) => {
                                event.preventDefault()
                                event.stopPropagation()
                                onDeleteToolPreset(preset.id)
                              }}
                            >
                              ×
                            </Button>
                          </div>
                        ))}
                      </div>
                    ) : (
                      <div className="text-[11px] text-[var(--text-dim)]">No presets yet.</div>
                    )}
                  </div>
                </DropdownMenuContent>
              </DropdownMenu>

              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <ComposerDropdownTrigger
                    aria-label={
                      modelOptions.find((option) => option.value === pulseModel)?.label ??
                      pulseModel
                    }
                  >
                    <Sparkles className="size-3.5 text-[var(--axon-primary)]" />
                  </ComposerDropdownTrigger>
                </DropdownMenuTrigger>
                <DropdownMenuContent
                  align="end"
                  className="w-56 border-[var(--border-subtle)] bg-[var(--glass-overlay)] text-[var(--text-primary)] backdrop-blur-xl"
                >
                  <DropdownMenuLabel className="px-2 py-1 text-[11px] uppercase tracking-[0.14em] text-[var(--text-dim)]">
                    Agent
                  </DropdownMenuLabel>
                  <DropdownMenuRadioGroup
                    value={pulseAgent}
                    onValueChange={(v) => onAgentChange(v as PulseAgent)}
                  >
                    {AGENT_OPTIONS.map((opt) => (
                      <DropdownMenuRadioItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </DropdownMenuRadioItem>
                    ))}
                  </DropdownMenuRadioGroup>
                  <DropdownMenuSeparator className="bg-[rgba(175,215,255,0.08)]" />
                  <DropdownMenuLabel className="px-2 py-1 text-[11px] uppercase tracking-[0.14em] text-[var(--text-dim)]">
                    Model
                  </DropdownMenuLabel>
                  <DropdownMenuRadioGroup value={pulseModel} onValueChange={onModelChange}>
                    {modelOptions.map((option) => (
                      <DropdownMenuRadioItem key={option.value} value={option.value}>
                        {option.label}
                      </DropdownMenuRadioItem>
                    ))}
                  </DropdownMenuRadioGroup>
                </DropdownMenuContent>
              </DropdownMenu>

              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <ComposerDropdownTrigger
                    aria-label={
                      permissionOptions.find((option) => option.value === pulsePermissionLevel)
                        ?.label ?? pulsePermissionLevel
                    }
                  >
                    <PermissionIcon className="size-3.5 text-[var(--axon-secondary-strong)]" />
                  </ComposerDropdownTrigger>
                </DropdownMenuTrigger>
                <DropdownMenuContent
                  align="end"
                  className="w-56 border-[var(--border-subtle)] bg-[var(--glass-overlay)] text-[var(--text-primary)] backdrop-blur-xl"
                >
                  <DropdownMenuLabel className="px-2 py-1 text-[11px] uppercase tracking-[0.14em] text-[var(--text-dim)]">
                    Permission
                  </DropdownMenuLabel>
                  <DropdownMenuRadioGroup
                    value={pulsePermissionLevel}
                    onValueChange={onPermissionChange}
                  >
                    {permissionOptions.map((option) => (
                      <DropdownMenuRadioItem key={option.value} value={option.value}>
                        {option.label}
                      </DropdownMenuRadioItem>
                    ))}
                  </DropdownMenuRadioGroup>
                </DropdownMenuContent>
              </DropdownMenu>
            </PromptInputTools>
          </div>

          <PromptInputButton
            aria-label="Attach files"
            onClick={() => fileInputRef.current?.click()}
          >
            <Paperclip className="size-4" />
          </PromptInputButton>

          <div className="shrink-0">
            <PromptInputSubmit disabled={isStreaming || !connected}>
              {isStreaming ? <Loader2 className="size-4 animate-spin" /> : undefined}
            </PromptInputSubmit>
          </div>
        </PromptInputFooter>
      </div>
    </PromptInput>
  )
})
