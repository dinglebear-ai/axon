'use client'

import type { AxonDensity } from './axon-shell-state-helpers'

const DENSITY_OPTIONS: { value: AxonDensity; label: string; description: string }[] = [
  { value: 'comfortable', label: 'Comfortable', description: 'Standard spacing and font sizes.' },
  { value: 'compact', label: 'Compact', description: 'Reduced padding and tighter text.' },
  {
    value: 'high',
    label: 'High Res',
    description: 'Maximum information density for large displays.',
  },
]

export function DensitySelector({
  density,
  onDensityChange,
}: {
  density: AxonDensity
  onDensityChange: (density: AxonDensity) => void
}) {
  return (
    <div className="mt-6">
      <span className="text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--text-dim)]">
        Display Density
      </span>
      <div className="mt-2 flex flex-col gap-2" role="radiogroup" aria-label="Display density">
        {DENSITY_OPTIONS.map(({ value, label, description }) => (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={density === value}
            onClick={() => onDensityChange(value)}
            className={`flex flex-col items-start rounded-lg border p-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring-color)] focus-visible:ring-offset-0 ${
              density === value
                ? 'border-[rgba(175,215,255,0.35)] bg-[rgba(175,215,255,0.08)] shadow-[0_0_12px_rgba(135,175,255,0.05)]'
                : 'border-[var(--border-subtle)] bg-[rgba(10,18,35,0.2)] hover:border-[rgba(175,215,255,0.2)] hover:bg-[rgba(135,175,255,0.04)]'
            }`}
          >
            <span
              className={`text-[13px] font-semibold ${density === value ? 'text-[var(--axon-primary-strong)]' : 'text-[var(--text-primary)]'}`}
            >
              {label}
            </span>
            <span className="mt-0.5 text-[11px] text-[var(--text-dim)]">{description}</span>
          </button>
        ))}
      </div>
    </div>
  )
}
