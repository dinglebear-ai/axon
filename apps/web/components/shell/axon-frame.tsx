'use client'

import type { ReactNode, RefObject } from 'react'
import type { NeuralCanvasHandle } from '@/components/neural-canvas'
import { NeuralCanvas } from '@/components/neural-canvas'
import type { NeuralCanvasProfile } from '@/lib/pulse/neural-canvas-presets'

export function AxonFrame({
  children,
  canvasRef,
  canvasProfile = 'current',
}: {
  children: ReactNode
  canvasRef?: RefObject<NeuralCanvasHandle | null>
  canvasProfile?: NeuralCanvasProfile
}) {
  return (
    <main className="relative min-h-dvh overflow-hidden bg-[#030817] text-[var(--text-primary)]">
      <NeuralCanvas ref={canvasRef} profile={canvasProfile} />
      <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_12%_8%,rgba(135,175,255,0.2),transparent_30%),radial-gradient(circle_at_86%_14%,rgba(255,135,175,0.1),transparent_24%),linear-gradient(180deg,rgba(3,8,23,0.14),rgba(3,8,23,0.62))]" />
      <div className="pointer-events-none absolute inset-0 bg-[linear-gradient(rgba(135,175,255,0.028)_1px,transparent_1px),linear-gradient(90deg,rgba(135,175,255,0.028)_1px,transparent_1px)] bg-[size:46px_46px] opacity-35" />
      <div className="relative z-[1]">{children}</div>
    </main>
  )
}
