'use client';

import { Eye, Pause, Pencil, Play, RefreshCw, Save, Trash2 } from 'lucide-react';
import { EmptyState, StatusBadge, SubsectionTitle } from '../../lib/panel-components';
import { normalizeJobStatus } from '../jobs/job-helpers';
import type { WatchSummary } from '../../lib/panel-types';
import { useWatchesPanel } from './use-watches';
import { watchEntryKey, watchScheduleLabel, watchStatusLabel, watchSummaryLabel } from './watch-helpers';

type WatchListRowProps = {
  entry: WatchSummary;
  busy: boolean;
  onPause: (watchId: string) => void;
  onResume: (watchId: string) => void;
  onEdit: (watchId: string) => void;
  onDelete: (watchId: string) => void;
};

export function WatchListRow({ entry, busy, onPause, onResume, onEdit, onDelete }: WatchListRowProps) {
  const status = entry.enabled ? normalizeJobStatus(entry.last_status ?? '') : 'skipped';

  return (
    <div className={`job-row watch-row ${status}`}>
      <div className="job-row-main">
        <strong title={entry.watch_id}>{entry.source_id || entry.watch_id}</strong>
        <small className="job-row-meta">
          <span>{watchScheduleLabel(entry.schedule)}</span>
          <span>{watchStatusLabel(entry)}</span>
        </small>
      </div>
      <div className="watch-row-actions">
        <StatusBadge status={status} />
        {entry.enabled ? (
          <button className="ghost icon-button" title="Pause watch" disabled={busy} onClick={() => onPause(entry.watch_id)}>
            <Pause aria-hidden="true" className="button-icon" />
          </button>
        ) : (
          <button className="ghost icon-button" title="Resume watch" disabled={busy} onClick={() => onResume(entry.watch_id)}>
            <Play aria-hidden="true" className="button-icon" />
          </button>
        )}
        <button className="ghost icon-button" title="Edit watch" disabled={busy} onClick={() => onEdit(entry.watch_id)}>
          <Pencil aria-hidden="true" className="button-icon" />
        </button>
        <button className="ghost icon-button danger" title="Delete watch" disabled={busy} onClick={() => onDelete(entry.watch_id)}>
          <Trash2 aria-hidden="true" className="button-icon" />
        </button>
      </div>
    </div>
  );
}

export function WatchesTab({ token, active }: { token: string; active: boolean }) {
  const {
    watchesResult,
    watchEntries,
    watchesLoading,
    watchesMessage,
    watchesUpdatedAt,
    refreshWatches,
    watchActionBusyId,
    pauseWatch,
    resumeWatch,
    deleteWatch,
    watchEditingId,
    watchEditSchedule, setWatchEditSchedule,
    watchEditCollection, setWatchEditCollection,
    watchEditBusy,
    watchEditError,
    startEditWatch,
    cancelEditWatch,
    submitEditWatch
  } = useWatchesPanel(token, active);

  return (
    <section className="stack-panel">
      <div className="section-heading">
        <div className="health-title">
          <div className={`status-orb ${watchesMessage ? 'error' : watchesResult ? 'ok' : 'warn'}`}>
            <Eye aria-hidden="true" className="status-glyph" />
          </div>
          <div>
            <p className="eyebrow">Watches</p>
            <h2>Source Watches</h2>
            <p>{watchSummaryLabel(watchesResult)}</p>
          </div>
        </div>
        <button className="ghost" onClick={() => void refreshWatches()} disabled={watchesLoading}>
          <RefreshCw aria-hidden="true" className={`button-icon ${watchesLoading ? 'spin' : ''}`} />
          {watchesLoading ? 'Refreshing' : 'Refresh'}
        </button>
      </div>
      <div className="summary-strip" aria-label="Watch summary">
        <span className="timestamp">{watchesUpdatedAt ? `Live ${watchesUpdatedAt}` : 'Not loaded yet'}</span>
      </div>
      {watchesMessage && <p className="error">{watchesMessage}</p>}
      <div className="status-panel">
        <SubsectionTitle icon={Eye} title="Source Watches" note="GET /v1/watches — schedule, status, and per-watch controls." />
        {watchEntries.length ? (
          <div className="job-list">
            {watchEntries.map((entry, index) => (
              <div key={watchEntryKey(entry, index)}>
                <WatchListRow
                  entry={entry}
                  busy={watchActionBusyId === entry.watch_id}
                  onPause={pauseWatch}
                  onResume={resumeWatch}
                  onEdit={startEditWatch}
                  onDelete={deleteWatch}
                />
                {watchEditingId === entry.watch_id && (
                  <div className="watch-edit-form">
                    <label>
                      Schedule (seconds)
                      <input
                        type="number"
                        min={1}
                        value={watchEditSchedule}
                        onChange={(event) => setWatchEditSchedule(event.target.value)}
                        placeholder={String(entry.schedule.every_seconds)}
                      />
                    </label>
                    <label>
                      Collection (optional)
                      <input
                        value={watchEditCollection}
                        onChange={(event) => setWatchEditCollection(event.target.value)}
                        placeholder="leave blank to keep current"
                      />
                    </label>
                    {watchEditError && <p className="error">{watchEditError}</p>}
                    <div className="editor-actions">
                      <button className="ghost" onClick={cancelEditWatch} disabled={watchEditBusy}>
                        Cancel
                      </button>
                      <button onClick={() => void submitEditWatch()} disabled={watchEditBusy}>
                        <Save aria-hidden="true" className="button-icon" />
                        {watchEditBusy ? 'Saving' : 'Save'}
                      </button>
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>
        ) : (
          <EmptyState loading={watchesLoading} text="No watches configured yet." />
        )}
      </div>
    </section>
  );
}
