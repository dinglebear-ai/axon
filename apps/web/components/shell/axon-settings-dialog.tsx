'use client'

import { Settings2 } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import type { NeuralCanvasProfile } from '@/lib/pulse/neural-canvas-presets'
import { CanvasProfileSelector } from './canvas-profile-selector'

export function AxonSettingsDialog({
  open,
  onOpenChange,
  canvasProfile,
  onCanvasProfileChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  canvasProfile: NeuralCanvasProfile
  onCanvasProfileChange: (profile: NeuralCanvasProfile) => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm border-[var(--border-subtle)] bg-[var(--glass-overlay)] text-[var(--text-primary)] backdrop-blur-xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-base">
            <Settings2 className="size-4" />
            Settings
          </DialogTitle>
          <DialogDescription className="sr-only">
            Application settings and preferences
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 pt-2">
          <CanvasProfileSelector
            canvasProfile={canvasProfile}
            onCanvasProfileChange={onCanvasProfileChange}
          />
        </div>
      </DialogContent>
    </Dialog>
  )
}
