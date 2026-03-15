import { Command, RefreshCw, ShieldCheck, Telescope } from 'lucide-react'
import { Button } from '@/components/ui/button'
import type { MissionControlModel } from '@/lib/cortex/mission-control-model'

interface ActionRailProps {
  model: MissionControlModel | null
  onRefresh: () => void
  onDoctorSweep: () => void
  onInspectSources: () => void
  onOpenJobsConsole: () => void
  jobsConsoleOpen: boolean
  railMessage: string | null
  doctorBusy: boolean
  sourcesBusy: boolean
  refreshing: boolean
}

export function ActionRail({
  model,
  onRefresh,
  onDoctorSweep,
  onInspectSources,
  onOpenJobsConsole,
  jobsConsoleOpen,
  railMessage,
  doctorBusy,
  sourcesBusy,
  refreshing,
}: ActionRailProps) {
  return (
    <aside className="space-y-4">
      <section className="axon-mission-card">
        <h2 className="text-sm font-semibold text-[var(--text-primary)]">Control Rail</h2>
        <div className="mt-3 grid gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={onRefresh}
            className="w-full justify-center gap-2 border-[var(--border-standard)] bg-[var(--surface-primary)] text-xs font-medium text-[var(--mc-accent-cyan)] hover:bg-[var(--surface-primary-active)]"
          >
            <RefreshCw className={`size-3.5 ${refreshing ? 'animate-spin' : ''}`} />
            Refresh Overview
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={onDoctorSweep}
            disabled={doctorBusy}
            className="w-full justify-center gap-2 border-[var(--border-subtle)] bg-[var(--surface-float)] text-xs text-[var(--text-secondary)] hover:border-[var(--border-standard)]"
          >
            <ShieldCheck className={`size-3.5 ${doctorBusy ? 'animate-pulse' : ''}`} />
            {doctorBusy ? 'Running Doctor Sweep...' : 'Run Doctor Sweep'}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={onInspectSources}
            disabled={sourcesBusy}
            className="w-full justify-center gap-2 border-[var(--border-subtle)] bg-[var(--surface-float)] text-xs text-[var(--text-secondary)] hover:border-[var(--border-standard)]"
          >
            <Telescope className={`size-3.5 ${sourcesBusy ? 'animate-pulse' : ''}`} />
            {sourcesBusy ? 'Inspecting Sources...' : 'Inspect Sources'}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={onOpenJobsConsole}
            className="w-full justify-center gap-2 border-[var(--border-subtle)] bg-[var(--surface-float)] text-xs text-[var(--text-secondary)] hover:border-[var(--border-standard)]"
          >
            <Command className="size-3.5" />
            {jobsConsoleOpen ? 'Jobs Console Open' : 'Open Jobs Console'}
          </Button>
        </div>
        {railMessage && <p className="mt-3 text-xs text-[var(--text-secondary)]">{railMessage}</p>}
      </section>

      <section className="axon-mission-card">
        <h3 className="text-xs uppercase tracking-[0.14em] text-[var(--text-dim)]">Snapshot</h3>
        <dl className="mt-3 grid gap-2 text-xs">
          <div className="flex items-center justify-between">
            <dt className="text-[var(--text-secondary)]">Collection</dt>
            <dd className="font-mono text-[var(--text-primary)]">
              {model?.sections.corpus.collection ?? 'cortex'}
            </dd>
          </div>
          <div className="flex items-center justify-between">
            <dt className="text-[var(--text-secondary)]">Domains</dt>
            <dd className="font-mono text-[var(--text-primary)]">{model?.kpis.domains ?? 0}</dd>
          </div>
          <div className="flex items-center justify-between">
            <dt className="text-[var(--text-secondary)]">Failed</dt>
            <dd className="font-mono text-[var(--status-failed)]">{model?.kpis.failed ?? 0}</dd>
          </div>
        </dl>
      </section>
    </aside>
  )
}
