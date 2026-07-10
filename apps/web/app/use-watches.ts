'use client';

import { useEffect, useMemo, useState } from 'react';
import { AxonClient } from '../lib/axon-client';
import type { WatchPage, WatchUpdateRequest } from './panel-types';
import { normalizeWatchEntries, parseScheduleSeconds, watchErrorMessage } from './watch-helpers';

/**
 * Standalone hook for the Watches tab (GET/PATCH/DELETE /v1/watches,
 * POST /v1/watches/{id}/{pause,resume}). Kept separate from
 * usePanelData()/use-panel-data.ts — that file is already at the monolith
 * line-count limit from the Sources tab (a sibling workstream), so watch
 * state and actions are self-contained here and consumed only by
 * watches-tab.tsx.
 */
export function useWatchesPanel(token: string, active: boolean) {
  const axonClient = useMemo(() => new AxonClient(), []);

  const [watchesResult, setWatchesResult] = useState<WatchPage | null>(null);
  const [watchesLoading, setWatchesLoading] = useState(false);
  const [watchesMessage, setWatchesMessage] = useState('');
  const [watchesUpdatedAt, setWatchesUpdatedAt] = useState('');
  const [watchActionBusyId, setWatchActionBusyId] = useState<string | null>(null);
  const [watchEditingId, setWatchEditingId] = useState<string | null>(null);
  const [watchEditSchedule, setWatchEditSchedule] = useState('');
  const [watchEditCollection, setWatchEditCollection] = useState('');
  const [watchEditBusy, setWatchEditBusy] = useState(false);
  const [watchEditError, setWatchEditError] = useState('');

  const watchEntries = useMemo(() => normalizeWatchEntries(watchesResult), [watchesResult]);

  useEffect(() => {
    if (!token || !active) return;
    void refreshWatches();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token, active]);

  async function refreshWatches(options: { quiet?: boolean } = {}) {
    if (!token) return;
    if (!options.quiet) setWatchesLoading(true);
    setWatchesMessage('');
    try {
      const result = (await axonClient.listWatches({ limit: 50 })) as WatchPage;
      setWatchesResult(result);
      setWatchesUpdatedAt(new Date().toLocaleTimeString());
    } catch (error) {
      setWatchesResult(null);
      setWatchesMessage(watchErrorMessage(error));
    } finally {
      if (!options.quiet) setWatchesLoading(false);
    }
  }

  async function pauseWatch(watchId: string) {
    setWatchActionBusyId(watchId);
    setWatchesMessage('');
    try {
      await axonClient.pauseWatch(watchId);
      await refreshWatches({ quiet: true });
    } catch (error) {
      setWatchesMessage(watchErrorMessage(error));
    } finally {
      setWatchActionBusyId(null);
    }
  }

  async function resumeWatch(watchId: string) {
    setWatchActionBusyId(watchId);
    setWatchesMessage('');
    try {
      await axonClient.resumeWatch(watchId);
      await refreshWatches({ quiet: true });
    } catch (error) {
      setWatchesMessage(watchErrorMessage(error));
    } finally {
      setWatchActionBusyId(null);
    }
  }

  async function deleteWatch(watchId: string) {
    if (typeof window !== 'undefined' && !window.confirm(`Delete watch ${watchId}? This cannot be undone.`)) {
      return;
    }
    setWatchActionBusyId(watchId);
    setWatchesMessage('');
    try {
      await axonClient.deleteWatch(watchId);
      if (watchEditingId === watchId) setWatchEditingId(null);
      await refreshWatches({ quiet: true });
    } catch (error) {
      setWatchesMessage(watchErrorMessage(error));
    } finally {
      setWatchActionBusyId(null);
    }
  }

  function startEditWatch(watchId: string) {
    const entry = watchEntries.find((item) => item.watch_id === watchId);
    setWatchEditingId(watchId);
    setWatchEditSchedule(entry ? String(entry.schedule.every_seconds) : '');
    setWatchEditCollection('');
    setWatchEditError('');
  }

  function cancelEditWatch() {
    setWatchEditingId(null);
    setWatchEditError('');
  }

  async function submitEditWatch() {
    if (!watchEditingId) return;
    const seconds = parseScheduleSeconds(watchEditSchedule);
    if (watchEditSchedule.trim() && seconds === null) {
      setWatchEditError('Schedule must be a positive number of seconds.');
      return;
    }
    setWatchEditBusy(true);
    setWatchEditError('');
    try {
      const body: WatchUpdateRequest = {};
      if (seconds !== null) body.schedule = { every_seconds: seconds };
      if (watchEditCollection.trim()) body.collection = watchEditCollection.trim();
      await axonClient.updateWatch(watchEditingId, body);
      setWatchEditingId(null);
      await refreshWatches({ quiet: true });
    } catch (error) {
      setWatchEditError(watchErrorMessage(error));
    } finally {
      setWatchEditBusy(false);
    }
  }

  return {
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
  };
}
