'use client'

import type { NeuralCanvasProfile } from '@/lib/pulse/neural-canvas-presets'

const CANVAS_PROFILES: { value: NeuralCanvasProfile; label: string }[] = [
  { value: 'current', label: 'Current' },
  { value: 'subtle', label: 'Subtle' },
  { value: 'cinematic', label: 'Cinematic' },
  { value: 'electric', label: 'Electric' },
  { value: 'zen', label: 'Zen' },
]

export function CanvasProfileSelector({
  canvasProfile,
  onCanvasProfileChange,
}: {
  canvasProfile: NeuralCanvasProfile
  onCanvasProfileChange: (profile: NeuralCanvasProfile) => void
}) {
  return (
    <div>
      <span className="text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--text-dim)]">
        Canvas Profile
      </span>
      <div className="mt-1.5 flex flex-wrap gap-1.5" role="radiogroup" aria-label="Canvas profile">
        {CANVAS_PROFILES.map(({ value, label }) => (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={canvasProfile === value}
            onClick={() => onCanvasProfileChange(value)}
            className={`rounded-md border px-3 py-1.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring-color)] focus-visible:ring-offset-0 ${
              canvasProfile === value
                ? 'border-[rgba(175,215,255,0.35)] bg-[rgba(175,215,255,0.12)] text-[var(--axon-primary-strong)]'
                : 'border-[var(--border-subtle)] text-[var(--text-dim)] hover:border-[rgba(175,215,255,0.2)] hover:text-[var(--text-secondary)]'
            }`}
          >
            {label}
          </button>
        ))}
      </div>
    </div>
  )
}
