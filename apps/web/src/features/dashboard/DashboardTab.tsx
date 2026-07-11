'use client';

import { Activity, Ban, Globe2, RefreshCw, Settings2 } from 'lucide-react';

import {
  CheckCard,
  DoctorCard,
  EmptyState,
  StatusGlyph,
  SubsectionTitle,
  SummaryPill,
  UrlCard,
  overallStatusLabel,
  summarizeChecks
} from '../../lib/panel-components';
import type {
  CheckSummary,
  DoctorService,
  PanelDoctorResponse,
  StackCheck,
  StackResponse,
  StackUrlCheck
} from '../../lib/panel-types';

export function DashboardTab({
  stack,
  stackLoading,
  stackStatus,
  stackUpdatedAt,
  doctor,
  doctorMessage,
  doctorUpdatedAt,
  urlSummary,
  runtimeChecks,
  skippedHostChecks,
  overallStatus,
  doctorServices,
  doctorSummary,
  refreshDashboard
}: {
  stack: StackResponse | null;
  stackLoading: boolean;
  stackStatus: string;
  stackUpdatedAt: string;
  doctor: PanelDoctorResponse | null;
  doctorMessage: string;
  doctorUpdatedAt: string;
  urlSummary: CheckSummary;
  runtimeChecks: StackCheck[];
  skippedHostChecks: StackCheck[];
  overallStatus: string;
  doctorServices: Array<DoctorService & { name: string }>;
  doctorSummary: CheckSummary;
  refreshDashboard: () => Promise<void>;
}) {
  return (
    <section className="stack-panel">
      <div className="section-heading">
        <div className="health-title">
          <div className={`status-orb ${overallStatus}`}>
            <StatusGlyph status={overallStatus} />
          </div>
          <div>
            <p className="eyebrow">Dashboard</p>
            <h2>Runtime Health</h2>
            <p>
              {stack
                ? `${overallStatusLabel(overallStatus)} · ${stack.server_url} · doctor ${doctor?.payload.all_ok ? 'clear' : 'checking'}`
                : stackLoading
                  ? 'Checking runtime'
                  : stackStatus || 'Runtime status unavailable'}
            </p>
          </div>
        </div>
        <button className="ghost" onClick={() => void refreshDashboard()} disabled={stackLoading}>
          <RefreshCw aria-hidden="true" className={`button-icon ${stackLoading ? 'spin' : ''}`} />
          {stackLoading ? 'Refreshing' : 'Refresh'}
        </button>
      </div>
      <div className="summary-strip" aria-label="Runtime health summary">
        <SummaryPill label="Service URLs" summary={urlSummary} />
        <SummaryPill label="Dependencies" summary={summarizeChecks(runtimeChecks)} />
        <SummaryPill label="Doctor" summary={doctorSummary} />
        <span className="timestamp">{doctorUpdatedAt || stackUpdatedAt ? `Live ${doctorUpdatedAt || stackUpdatedAt}` : 'Starting live view'}</span>
      </div>
      {(stackStatus || doctorMessage) && <p className="error">{stackStatus || doctorMessage}</p>}
      <div className="runtime-grid">
        <div className="runtime-primary">
          <SubsectionTitle icon={Globe2} title="Service URLs" note="Reachability from this Axon server." />
          {stack?.urls?.length ? (
            <div className="url-list" aria-label="Service URL reachability">
              {stack.urls.map((urlCheck: StackUrlCheck) => (
                <UrlCard check={urlCheck} key={urlCheck.label} />
              ))}
            </div>
          ) : (
            <EmptyState loading={stackLoading} text="No URL checks returned." />
          )}
        </div>
        <div className="runtime-secondary">
          <SubsectionTitle icon={Settings2} title="Runtime Dependencies" note="Server-context checks." />
          {runtimeChecks.length ? (
            <div className="dependency-list">
              {runtimeChecks.map((check) => (
                <CheckCard check={check} key={check.label} />
              ))}
            </div>
          ) : (
            <EmptyState loading={stackLoading} text="No dependency checks returned." />
          )}
          {skippedHostChecks.length > 0 && (
            <div className="skip-strip">
              <div>
                <Ban aria-hidden="true" className="heading-icon" />
                <strong>Host-only checks unavailable</strong>
              </div>
              <p>{skippedHostChecks.map((check) => check.label).join(' · ')}</p>
            </div>
          )}
        </div>
      </div>
      <div className="doctor-panel">
        <SubsectionTitle icon={Activity} title="Doctor" note="Live `axon doctor` service report." />
        {doctorServices.length ? (
          <div className="doctor-grid">
            {doctorServices.map((service) => (
              <DoctorCard service={service} key={service.name} />
            ))}
          </div>
        ) : (
          <EmptyState loading={stackLoading} text="No doctor report returned." />
        )}
      </div>
    </section>
  );
}
