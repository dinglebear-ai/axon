'use client'

import { useCallback, useEffect, useRef, useState } from 'react'
import { useAxonWs } from '@/hooks/use-axon-ws'
import type { AggregateStats, ContainerStats, WsServerMsg } from '@/lib/ws-protocol'

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes.toFixed(0)}B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)}MB`
}

interface StatsData {
  aggregate: AggregateStats
  containers: Record<string, ContainerStats>
  container_count: number
}

interface DockerStatsProps {
  onStats?: (data: StatsData) => void
  /** When false the component skips React state updates (no re-renders) but
   *  still forwards data via onStats and keeps the status label current. */
  visible?: boolean
}

export function DockerStats({ onStats, visible = false }: DockerStatsProps) {
  const { subscribe, updateStatusLabel } = useAxonWs()
  const [data, setData] = useState<StatsData | null>(null)
  const lastStatusLabelAtRef = useRef(0)
  const visibleRef = useRef(visible)
  visibleRef.current = visible

  const stableOnStats = useCallback((d: StatsData) => onStats?.(d), [onStats])

  useEffect(() => {
    return subscribe((msg: WsServerMsg) => {
      if (msg.type !== 'stats') return
      const statsData: StatsData = {
        aggregate: msg.aggregate,
        containers: msg.containers,
        container_count: msg.container_count,
      }

      // Only trigger a React re-render when the panel is actually visible.
      // The onStats callback and status-label update run regardless so that
      // the neural canvas and toolbar stay current.
      if (visibleRef.current) {
        setData(statsData)
      }
      stableOnStats(statsData)

      // Throttle status-label updates to avoid high-frequency context churn.
      const now = Date.now()
      if (now - lastStatusLabelAtRef.current >= 1000) {
        lastStatusLabelAtRef.current = now
        updateStatusLabel(
          `LIVE ${msg.container_count}\u00d7 CPU ${msg.aggregate.cpu_percent.toFixed(0)}%`,
        )
      }
    })
  }, [subscribe, stableOnStats, updateStatusLabel])

  if (!data) {
    return (
      <div className="p-6 text-center text-[var(--text-muted)] text-sm">
        Waiting for Docker stats...
      </div>
    )
  }

  const { aggregate: agg, containers, container_count } = data
  const names = Object.keys(containers).sort()

  return (
    <div className="space-y-4">
      {/* Aggregate stats grid */}
      <div className="grid grid-cols-4 gap-3">
        <StatCard value={String(container_count)} label="Containers" />
        <StatCard value={`${agg.cpu_percent.toFixed(1)}%`} label="Total CPU" />
        <StatCard value={`${agg.avg_memory_percent.toFixed(1)}%`} label="Avg Memory" />
        <StatCard value={`${formatBytes(agg.total_net_io_rate)}/s`} label="Net I/O" />
      </div>

      {/* Per-container details */}
      {names.length > 0 && (
        <div className="space-y-1 font-mono text-xs">
          {names.map((name) => {
            // name comes from Object.keys(containers), so containers[name] is always defined
            const m = containers[name]!
            const shortName = name.replace(/^axon-/, '')
            return (
              <div
                key={name}
                className="flex items-center gap-3 px-2 py-1 rounded bg-[var(--surface-base)]"
              >
                <span className="text-[var(--text-primary)] font-medium min-w-[80px] truncate">
                  {shortName}
                </span>
                <span className="text-[var(--text-muted)]">CPU {m.cpu_percent.toFixed(1)}%</span>
                <span className="text-[var(--text-muted)]">
                  MEM {m.memory_usage_mb.toFixed(0)}MB/{m.memory_limit_mb.toFixed(0)}MB
                </span>
                <span className="text-[var(--text-muted)]">
                  NET {'\u2191'}
                  {formatBytes(m.net_tx_rate)}/s {'\u2193'}
                  {formatBytes(m.net_rx_rate)}/s
                </span>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

function StatCard({ value, label }: { value: string; label: string }) {
  return (
    <div className="rounded-lg border border-[var(--border-subtle)] bg-[var(--surface-base)] p-3 text-center">
      <div className="text-lg font-semibold text-[var(--text-primary)]">{value}</div>
      <div className="text-xs text-[var(--text-muted)]">{label}</div>
    </div>
  )
}
