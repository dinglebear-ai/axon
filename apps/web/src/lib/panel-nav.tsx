'use client';

import { Activity, Brain, Database, Eye, FileCog, ListChecks } from 'lucide-react';

import type { PanelTab } from './panel-types';

export function PanelNav({
  activePanelTab,
  setActivePanelTab,
  configDirty,
  envDirty,
  activeJobCount,
}: {
  activePanelTab: PanelTab;
  setActivePanelTab: (tab: PanelTab) => void;
  configDirty: boolean;
  envDirty: boolean;
  activeJobCount: number;
}) {
  return (
    <nav className="panel-tabs" aria-label="Admin panel sections">
      <button
        className={activePanelTab === 'dashboard' ? 'selected' : ''}
        onClick={() => setActivePanelTab('dashboard')}
      >
        <Activity aria-hidden="true" className="button-icon" />
        Dashboard
      </button>
      <button
        className={activePanelTab === 'configurator' ? 'selected' : ''}
        onClick={() => setActivePanelTab('configurator')}
      >
        <FileCog aria-hidden="true" className="button-icon" />
        Configurator
        {(configDirty || envDirty) && <span className="dirty-dot" aria-label="Modified" />}
      </button>
      <button
        className={activePanelTab === 'jobs' ? 'selected' : ''}
        onClick={() => setActivePanelTab('jobs')}
      >
        <ListChecks aria-hidden="true" className="button-icon" />
        Jobs
        {activeJobCount > 0 && <span className="dirty-dot" aria-label="Active jobs" />}
      </button>
      <button
        className={activePanelTab === 'sources' ? 'selected' : ''}
        onClick={() => setActivePanelTab('sources')}
      >
        <Database aria-hidden="true" className="button-icon" />
        Sources
      </button>
      <button
        className={activePanelTab === 'watches' ? 'selected' : ''}
        onClick={() => setActivePanelTab('watches')}
      >
        <Eye aria-hidden="true" className="button-icon" />
        Watches
      </button>
      <button
        className={activePanelTab === 'memory' ? 'selected' : ''}
        onClick={() => setActivePanelTab('memory')}
      >
        <Brain aria-hidden="true" className="button-icon" />
        Memory
      </button>
    </nav>
  );
}
