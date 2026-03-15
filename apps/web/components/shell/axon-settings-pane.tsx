'use client'

import { Settings2, Shield, Timer } from 'lucide-react'
import { memo, useCallback } from 'react'
import { toast } from 'sonner'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import type { NeuralCanvasProfile } from '@/lib/pulse/neural-canvas-presets'
import { CanvasProfileSelector } from './canvas-profile-selector'

export const AxonSettingsPane = memo(function AxonSettingsPane({
  canvasProfile,
  onCanvasProfileChange,
  enableFs,
  onEnableFsChange,
  enableTerminal,
  onEnableTerminalChange,
  permissionTimeoutSecs,
  onPermissionTimeoutSecsChange,
  adapterTimeoutSecs,
  onAdapterTimeoutSecsChange,
}: {
  canvasProfile: NeuralCanvasProfile
  onCanvasProfileChange: (profile: NeuralCanvasProfile) => void
  enableFs: boolean
  onEnableFsChange: (val: boolean) => void
  enableTerminal: boolean
  onEnableTerminalChange: (val: boolean) => void
  permissionTimeoutSecs: number | null
  onPermissionTimeoutSecsChange: (val: number | null) => void
  adapterTimeoutSecs: number | null
  onAdapterTimeoutSecsChange: (val: number | null) => void
}) {
  const handleFsChange = useCallback(
    (val: boolean) => {
      onEnableFsChange(val)
      toast.success(`Filesystem access ${val ? 'enabled' : 'disabled'}`)
    },
    [onEnableFsChange],
  )

  const handleTerminalChange = useCallback(
    (val: boolean) => {
      onEnableTerminalChange(val)
      toast.success(`Terminal access ${val ? 'enabled' : 'disabled'}`)
    },
    [onEnableTerminalChange],
  )

  const handlePermissionTimeout = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      if (!e.target.value) {
        onPermissionTimeoutSecsChange(null)
        return
      }
      const n = parseInt(e.target.value, 10)
      if (Number.isNaN(n)) return
      onPermissionTimeoutSecsChange(n)
    },
    [onPermissionTimeoutSecsChange],
  )

  const handleAdapterTimeout = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      if (!e.target.value) {
        onAdapterTimeoutSecsChange(null)
        return
      }
      const n = parseInt(e.target.value, 10)
      if (Number.isNaN(n)) return
      onAdapterTimeoutSecsChange(n)
    },
    [onAdapterTimeoutSecsChange],
  )

  const permissionOutOfRange =
    permissionTimeoutSecs !== null && (permissionTimeoutSecs < 1 || permissionTimeoutSecs > 3600)
  const adapterOutOfRange =
    adapterTimeoutSecs !== null && (adapterTimeoutSecs < 1 || adapterTimeoutSecs > 86400)

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border-subtle)] px-3 py-2.5">
        <Settings2 className="size-4 text-[var(--axon-primary-strong)]" />
        <span className="text-[13px] font-semibold text-[var(--text-primary)]">Settings</span>
      </div>
      <div className="flex-1 space-y-6 overflow-y-auto px-3 py-3">
        {/* Appearance */}
        <section className="space-y-3">
          <div className="flex items-center gap-2 text-[var(--text-muted)]">
            <Settings2 className="size-3.5" />
            <h3 className="text-xs font-semibold uppercase tracking-wider">Appearance</h3>
          </div>
          <div className="space-y-4">
            <CanvasProfileSelector
              canvasProfile={canvasProfile}
              onCanvasProfileChange={onCanvasProfileChange}
            />
          </div>
        </section>

        {/* Agent Capabilities */}
        <section className="space-y-3">
          <div className="flex items-center gap-2 text-[var(--text-muted)]">
            <Shield className="size-3.5" />
            <h3 className="text-xs font-semibold uppercase tracking-wider">Capabilities</h3>
          </div>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5 pr-2">
                <Label htmlFor="enable-fs" className="cursor-pointer text-[13px]">
                  Filesystem Access
                </Label>
                <p className="text-[11px] text-[var(--text-dim)]">
                  Allow agent to read/write local files
                </p>
              </div>
              <Switch
                id="enable-fs"
                checked={enableFs}
                onCheckedChange={handleFsChange}
                aria-label="Toggle filesystem access"
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5 pr-2">
                <Label htmlFor="enable-terminal" className="cursor-pointer text-[13px]">
                  Terminal Access
                </Label>
                <p className="text-[11px] text-[var(--text-dim)]">
                  Allow agent to execute shell commands
                </p>
              </div>
              <Switch
                id="enable-terminal"
                checked={enableTerminal}
                onCheckedChange={handleTerminalChange}
                aria-label="Toggle terminal access"
              />
            </div>
          </div>
        </section>

        {/* Timeouts */}
        <section className="space-y-3">
          <div className="flex items-center gap-2 text-[var(--text-muted)]">
            <Timer className="size-3.5" />
            <h3 className="text-xs font-semibold uppercase tracking-wider">Timeouts</h3>
          </div>
          <div className="space-y-3">
            <div className="space-y-2">
              <Label htmlFor="permission-timeout" className="text-[13px]">
                Permission Timeout (seconds)
              </Label>
              <Input
                id="permission-timeout"
                type="number"
                min={1}
                max={3600}
                inputMode="numeric"
                placeholder="Default: 60"
                value={permissionTimeoutSecs ?? ''}
                onChange={handlePermissionTimeout}
                aria-invalid={permissionOutOfRange || undefined}
                className={`h-8 text-xs bg-[var(--surface-sunken)] border-[var(--border-subtle)] focus-visible:ring-1 focus-visible:ring-[var(--axon-primary-strong)] ${
                  permissionOutOfRange ? 'border-destructive focus-visible:ring-destructive/50' : ''
                }`}
              />
              {permissionOutOfRange && (
                <p className="text-[10px] text-destructive">
                  Value must be between 1 and 3600 seconds
                </p>
              )}
              <p className="text-[10px] text-[var(--text-dim)]">
                How long to wait for your approval before auto-cancelling
              </p>
            </div>
            <div className="space-y-2">
              <Label htmlFor="adapter-timeout" className="text-[13px]">
                Adapter Timeout (seconds)
              </Label>
              <Input
                id="adapter-timeout"
                type="number"
                min={1}
                max={86400}
                inputMode="numeric"
                placeholder="Default: 300"
                value={adapterTimeoutSecs ?? ''}
                onChange={handleAdapterTimeout}
                aria-invalid={adapterOutOfRange || undefined}
                className={`h-8 text-xs bg-[var(--surface-sunken)] border-[var(--border-subtle)] focus-visible:ring-1 focus-visible:ring-[var(--axon-primary-strong)] ${
                  adapterOutOfRange ? 'border-destructive focus-visible:ring-destructive/50' : ''
                }`}
              />
              {adapterOutOfRange && (
                <p className="text-[10px] text-destructive">
                  Value must be between 1 and 86400 seconds
                </p>
              )}
              <p className="text-[10px] text-[var(--text-dim)]">
                Maximum execution time for any single agent request
              </p>
            </div>
          </div>
        </section>
      </div>
    </div>
  )
})
