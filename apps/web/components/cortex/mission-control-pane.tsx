'use client'

import { AlertCircle, BrainCircuit } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { apiFetch } from '@/lib/api-fetch'
import {
  buildMissionControlModel,
  type MissionControlModel,
} from '@/lib/cortex/mission-control-model'
import type { CortexOverview } from '@/lib/cortex/overview-normalize'
import type { DoctorResult, SourcesPagedResult, SourcesResult } from '@/lib/result-types'
import { ActionRail } from './mission-control/action-rail'
import { CorpusMap } from './mission-control/corpus-map'
import { HealthStrip } from './mission-control/health-strip'
import { HeroKpis } from './mission-control/hero-kpis'
import { JobsConsole } from './mission-control/jobs-console'
import { QueueRadar } from './mission-control/queue-radar'

interface OverviewResponse {
  ok: boolean
  data?: CortexOverview
  error?: string
}

interface ApiResponse<T> {
  ok: boolean
  data?: T
  error?: string
}

function toSourceCount(payload: SourcesResult): number {
  if (payload && typeof payload === 'object' && 'urls' in payload) {
    return (payload as SourcesPagedResult).count ?? (payload as SourcesPagedResult).urls.length
  }
  return Object.keys(payload ?? {}).length
}

export function MissionControlPane() {
  const [model, setModel] = useState<MissionControlModel | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [refreshing, setRefreshing] = useState(false)
  const [doctorBusy, setDoctorBusy] = useState(false)
  const [sourcesBusy, setSourcesBusy] = useState(false)
  const [railMessage, setRailMessage] = useState<string | null>(null)
  const [jobsConsoleOpen, setJobsConsoleOpen] = useState(false)
  const jobsConsoleRef = useRef<HTMLElement | null>(null)

  const load = useCallback(async (manual = false) => {
    const controller = new AbortController()
    const timer = setTimeout(() => controller.abort('timeout'), 12_000)
    if (manual) setRefreshing(true)
    setError(null)
    try {
      const res = await apiFetch('/api/cortex/overview', { signal: controller.signal })
      const json = (await res.json()) as OverviewResponse
      if (!json.ok || !json.data) {
        throw new Error(json.error ?? 'Failed to load mission control overview')
      }
      setModel(buildMissionControlModel(json.data))
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      if (message.includes('aborted') || message.includes('AbortError')) {
        setError('Cortex overview timed out. Try Refresh to retry.')
      } else {
        setError(message)
      }
    } finally {
      clearTimeout(timer)
      setLoading(false)
      setRefreshing(false)
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  const runDoctorSweep = useCallback(async () => {
    setDoctorBusy(true)
    setRailMessage('Running doctor sweep…')
    try {
      const res = await apiFetch('/api/cortex/doctor')
      const json = (await res.json()) as ApiResponse<DoctorResult>
      if (!json.ok || !json.data) {
        throw new Error(json.error ?? 'Doctor sweep failed')
      }
      const unhealthy = Object.values(json.data.services ?? {}).filter((svc) => !svc.ok).length
      setRailMessage(
        json.data.all_ok
          ? 'Doctor sweep complete: all services healthy.'
          : `Doctor sweep complete: ${unhealthy} unhealthy service(s).`,
      )
      await load(true)
    } catch (err) {
      setRailMessage(`Doctor sweep failed: ${err instanceof Error ? err.message : String(err)}`)
    } finally {
      setDoctorBusy(false)
    }
  }, [load])

  const inspectSources = useCallback(async () => {
    setSourcesBusy(true)
    setRailMessage('Inspecting indexed sources…')
    try {
      const res = await apiFetch('/api/cortex/sources')
      const json = (await res.json()) as ApiResponse<SourcesResult>
      if (!json.ok || !json.data) {
        throw new Error(json.error ?? 'Source inspection failed')
      }
      setRailMessage(`Indexed sources discovered: ${toSourceCount(json.data)}.`)
      await load(true)
    } catch (err) {
      setRailMessage(
        `Source inspection failed: ${err instanceof Error ? err.message : String(err)}`,
      )
    } finally {
      setSourcesBusy(false)
    }
  }, [load])

  const openJobsConsole = useCallback(() => {
    setJobsConsoleOpen(true)
    setRailMessage('Jobs console opened.')
    requestAnimationFrame(() => {
      jobsConsoleRef.current?.scrollIntoView?.({ behavior: 'smooth', block: 'start' })
    })
  }, [])

  return (
    <section className="axon-mission-control mx-auto w-full max-w-7xl p-4 md:p-5">
      <header className="mb-3 flex items-center gap-1.5">
        <BrainCircuit className="size-4.5 text-[var(--mc-accent-cyan)]" />
        <h1 className="text-xl font-semibold tracking-tight text-[var(--text-primary)]">
          Mission Control
        </h1>
      </header>

      {loading && (
        <div
          role="status"
          className="space-y-3 animate-fade-in"
          aria-busy="true"
          aria-label="Loading mission telemetry"
        >
          {/* Skeleton: KPI strip */}
          <div className="axon-mission-card grid grid-cols-2 gap-3 sm:grid-cols-4">
            {Array.from({ length: 4 }).map((_, i) => (
              <div
                key={i}
                className="space-y-2 rounded-lg border border-[rgba(175,215,255,0.08)] bg-[rgba(7,12,26,0.4)] p-3"
              >
                <div
                  className="h-2 w-16 animate-pulse rounded-full bg-[rgba(175,215,255,0.1)]"
                  style={{ animationDelay: `${i * 75}ms` }}
                />
                <div
                  className="h-5 w-20 animate-pulse rounded bg-[rgba(175,215,255,0.12)]"
                  style={{ animationDelay: `${i * 75 + 50}ms` }}
                />
              </div>
            ))}
          </div>
          {/* Skeleton: Health strip */}
          <div className="axon-mission-card grid grid-cols-3 gap-2 sm:grid-cols-6">
            {Array.from({ length: 6 }).map((_, i) => (
              <div
                key={i}
                className="flex items-center gap-2 rounded-md border border-[rgba(175,215,255,0.06)] bg-[rgba(7,12,26,0.3)] px-2 py-2"
              >
                <div
                  className="size-2 animate-pulse rounded-full bg-[rgba(175,215,255,0.15)]"
                  style={{ animationDelay: `${i * 50}ms` }}
                />
                <div
                  className="h-2 w-14 animate-pulse rounded-full bg-[rgba(175,215,255,0.08)]"
                  style={{ animationDelay: `${i * 50 + 30}ms` }}
                />
              </div>
            ))}
          </div>
          {/* Skeleton: Queue / content area */}
          <div className="axon-mission-card space-y-2 rounded-lg border border-[rgba(175,215,255,0.06)] bg-[rgba(7,12,26,0.3)] p-4">
            <div className="h-3 w-28 animate-pulse rounded-full bg-[rgba(175,215,255,0.1)]" />
            <div
              className="h-2.5 w-full animate-pulse rounded-full bg-[rgba(175,215,255,0.06)]"
              style={{ animationDelay: '100ms' }}
            />
            <div
              className="h-2.5 w-4/5 animate-pulse rounded-full bg-[rgba(175,215,255,0.05)]"
              style={{ animationDelay: '175ms' }}
            />
            <div
              className="h-2.5 w-3/5 animate-pulse rounded-full bg-[rgba(175,215,255,0.04)]"
              style={{ animationDelay: '250ms' }}
            />
          </div>
        </div>
      )}

      {!loading && error && (
        <div className="axon-mission-card flex items-start gap-2 text-sm text-[var(--status-failed)]">
          <AlertCircle className="size-4" />
          <span>{error}</span>
        </div>
      )}

      {!loading && !error && (
        <div className="space-y-3">
          <div className="axon-mission-grid grid gap-3 lg:grid-cols-[1.2fr_0.8fr]">
            <div className="space-y-3">
              <HeroKpis model={model} />
              <HealthStrip model={model} />
              <QueueRadar model={model} />
              <CorpusMap model={model} />
            </div>
            <ActionRail
              model={model}
              onRefresh={() => void load(true)}
              onDoctorSweep={() => void runDoctorSweep()}
              onInspectSources={() => void inspectSources()}
              onOpenJobsConsole={openJobsConsole}
              jobsConsoleOpen={jobsConsoleOpen}
              railMessage={railMessage}
              doctorBusy={doctorBusy}
              sourcesBusy={sourcesBusy}
              refreshing={refreshing}
            />
          </div>

          {jobsConsoleOpen && <JobsConsole model={model} panelRef={jobsConsoleRef} />}
        </div>
      )}
    </section>
  )
}
