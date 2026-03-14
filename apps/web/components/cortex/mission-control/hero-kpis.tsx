import type { MissionControlModel } from '@/lib/cortex/mission-control-model'

function KpiTile({
  label,
  value,
  tone = 'default',
}: {
  label: string
  value: string
  tone?: 'default' | 'alert' | 'good'
}) {
  const toneClass =
    tone === 'alert'
      ? 'text-[var(--mc-accent-coral)]'
      : tone === 'good'
        ? 'text-[var(--status-completed)]'
        : 'text-[var(--mc-accent-cyan)]'

  return (
    <article className="axon-mission-card">
      <p className="text-[10px] uppercase tracking-[0.12em] text-[var(--text-dim)]">{label}</p>
      <p className={`mt-1.5 font-mono text-[1.5rem] font-semibold leading-[1.1] ${toneClass}`}>
        {value}
      </p>
    </article>
  )
}

export function HeroKpis({ model }: { model: MissionControlModel | null }) {
  return (
    <section className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-label="Mission KPIs">
      <KpiTile label="Queue Pressure" value={model?.kpis.queuePressure ?? 'unknown'} />
      <KpiTile
        label="Reliability"
        value={model?.kpis.reliability ?? 'unknown'}
        tone={model?.kpis.reliability === 'degraded' ? 'alert' : 'good'}
      />
      <KpiTile label="Active Work" value={String(model?.kpis.activeWork ?? 0)} />
      <KpiTile label="Vectors" value={String(model?.kpis.vectors ?? 0)} />
    </section>
  )
}
