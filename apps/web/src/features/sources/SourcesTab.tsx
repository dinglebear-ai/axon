'use client';

import { Database, Play, RefreshCw, UploadCloud } from 'lucide-react';

import { EmptyState, SourceListRow, SourceSubmitResultCard, SubsectionTitle } from '../../lib/panel-components';
import type { SourceListEntry, SourcesListResult } from '../../lib/panel-types';
import type { SourceResult } from '../../api/axon-client';
import { SOURCE_FAMILY_OPTIONS, sourceEntryKey, sourcesSummaryLabel } from './source-helpers';

export function SourcesTab({
  sourceEntries,
  sourcesResult,
  sourcesLoading,
  sourcesMessage,
  sourcesUpdatedAt,
  refreshSources,
  sourceFormValue,
  setSourceFormValue,
  sourceFormFamily,
  setSourceFormFamily,
  sourceFormEmbed,
  setSourceFormEmbed,
  sourceFormMaxPages,
  setSourceFormMaxPages,
  sourceSubmitBusy,
  sourceSubmitResult,
  sourceSubmitError,
  submitSourceForm
}: {
  sourceEntries: SourceListEntry[];
  sourcesResult: SourcesListResult | null;
  sourcesLoading: boolean;
  sourcesMessage: string;
  sourcesUpdatedAt: string;
  refreshSources: () => Promise<void>;
  sourceFormValue: string;
  setSourceFormValue: (value: string) => void;
  sourceFormFamily: string;
  setSourceFormFamily: (value: string) => void;
  sourceFormEmbed: boolean;
  setSourceFormEmbed: (value: boolean) => void;
  sourceFormMaxPages: string;
  setSourceFormMaxPages: (value: string) => void;
  sourceSubmitBusy: boolean;
  sourceSubmitResult: SourceResult | null;
  sourceSubmitError: string;
  submitSourceForm: () => Promise<void>;
}) {
  return (
    <section className="stack-panel">
      <div className="section-heading">
        <div className="health-title">
          <div className={`status-orb ${sourcesMessage ? 'error' : sourcesResult ? 'ok' : 'warn'}`}>
            <Database aria-hidden="true" className="status-glyph" />
          </div>
          <div>
            <p className="eyebrow">Sources</p>
            <h2>Indexed Sources</h2>
            <p>{sourcesSummaryLabel(sourcesResult)}</p>
          </div>
        </div>
        <button className="ghost" onClick={() => void refreshSources()} disabled={sourcesLoading}>
          <RefreshCw aria-hidden="true" className={`button-icon ${sourcesLoading ? 'spin' : ''}`} />
          {sourcesLoading ? 'Refreshing' : 'Refresh'}
        </button>
      </div>
      <div className="summary-strip" aria-label="Source summary">
        <span className="timestamp">{sourcesUpdatedAt ? `Live ${sourcesUpdatedAt}` : 'Not loaded yet'}</span>
      </div>
      {sourcesMessage && <p className="error">{sourcesMessage}</p>}
      <div className="status-grid">
        <div className="status-panel">
          <SubsectionTitle icon={Database} title="Indexed Sources" note="GET /v1/sources — family, adapter, and chunk counts." />
          {sourceEntries.length ? (
            <div className="job-list">
              {sourceEntries.map((entry, index) => (
                <SourceListRow entry={entry} key={sourceEntryKey(entry, index)} />
              ))}
            </div>
          ) : (
            <EmptyState loading={sourcesLoading} text="No sources indexed yet." />
          )}
        </div>
        <div className="status-panel command-card">
          <SubsectionTitle icon={UploadCloud} title="Submit Source" note="POST /v1/sources — acquires and indexes a URL, repo, feed, or path." />
          <label>
            Source URL or target
            <input
              value={sourceFormValue}
              onChange={(event) => setSourceFormValue(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') void submitSourceForm();
              }}
              placeholder="https://example.com/docs"
            />
          </label>
          <label>
            Family
            <select value={sourceFormFamily} onChange={(event) => setSourceFormFamily(event.target.value)}>
              {SOURCE_FAMILY_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Max pages (optional)
            <input
              type="number"
              min={0}
              value={sourceFormMaxPages}
              onChange={(event) => setSourceFormMaxPages(event.target.value)}
              placeholder="unbounded"
            />
          </label>
          <label className="checkbox-field">
            <input
              type="checkbox"
              checked={sourceFormEmbed}
              onChange={(event) => setSourceFormEmbed(event.target.checked)}
            />
            Embed into Qdrant
          </label>
          <button onClick={() => void submitSourceForm()} disabled={sourceSubmitBusy || !sourceFormValue.trim()}>
            <Play aria-hidden="true" className="button-icon" />
            {sourceSubmitBusy ? 'Submitting' : 'Submit source'}
          </button>
          {sourceSubmitError && <p className="error">{sourceSubmitError}</p>}
          {sourceSubmitResult && <SourceSubmitResultCard result={sourceSubmitResult} />}
        </div>
      </div>
    </section>
  );
}
