'use client';

import { Activity, Command, ListChecks, RefreshCw, Terminal } from 'lucide-react';

import { commandExamples } from '../../lib/command-format';
import { EmptyState, JobRow, SubsectionTitle, SummaryPill } from '../../lib/panel-components';
import type { PanelStatusResponse, ServiceJob } from '../../lib/panel-types';
import { jobSummary } from './job-helpers';

export function JobsTab({
  activeJobs,
  liveJobs,
  axonStatus,
  statusMessage,
  statusUpdatedAt,
  stackLoading,
  refreshAxonStatus,
  setPaletteOpen,
  setCommandInput
}: {
  activeJobs: ServiceJob[];
  liveJobs: ServiceJob[];
  axonStatus: PanelStatusResponse | null;
  statusMessage: string;
  statusUpdatedAt: string;
  stackLoading: boolean;
  refreshAxonStatus: () => Promise<void>;
  setPaletteOpen: (open: boolean) => void;
  setCommandInput: (value: string) => void;
}) {
  return (
    <section className="stack-panel">
      <div className="section-heading">
        <div className="health-title">
          <div className={`status-orb ${activeJobs.length ? 'warn' : 'ok'}`}>
            <ListChecks aria-hidden="true" className="status-glyph" />
          </div>
          <div>
            <p className="eyebrow">Jobs</p>
            <h2>Axon Status</h2>
            <p>{activeJobs.length} active jobs · {liveJobs.length} recent rows</p>
          </div>
        </div>
        <button className="ghost" onClick={() => void refreshAxonStatus()} disabled={stackLoading}>
          <RefreshCw aria-hidden="true" className={`button-icon ${stackLoading ? 'spin' : ''}`} />
          Refresh
        </button>
      </div>
      <div className="summary-strip" aria-label="Job summary">
        <SummaryPill label="Active jobs" summary={jobSummary(activeJobs)} />
        <span className="timestamp">{statusUpdatedAt ? `Live ${statusUpdatedAt}` : 'Starting live view'}</span>
      </div>
      {statusMessage && <p className="error">{statusMessage}</p>}
      <div className="status-grid">
        <div className="status-panel">
          <SubsectionTitle icon={Activity} title="Axon Status" note="Queue totals and recent jobs." />
          <div className="job-total-grid">
            {Object.entries(axonStatus?.totals ?? {}).map(([label, value]) => (
              <div className="job-total" key={label}>
                <span>{label}</span>
                <strong>{value}</strong>
              </div>
            ))}
          </div>
          {liveJobs.length ? (
            <div className="job-list">
              {liveJobs.slice(0, 10).map((job) => (
                <JobRow job={job} key={job.id} />
              ))}
            </div>
          ) : (
            <EmptyState loading={stackLoading} text="No recent jobs returned." />
          )}
        </div>
        <div className="status-panel command-card">
          <SubsectionTitle icon={Command} title="Command Palette" note="Run Axon commands from the browser." />
          <button className="command-open" onClick={() => setPaletteOpen(true)}>
            <Terminal aria-hidden="true" className="button-icon" />
            Open command palette
            <kbd>⌘K</kbd>
          </button>
          <div className="command-examples">
            {commandExamples.map((example) => (
              <button
                className="ghost"
                key={example}
                onClick={() => {
                  setCommandInput(example);
                  setPaletteOpen(true);
                }}
              >
                {example}
              </button>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
