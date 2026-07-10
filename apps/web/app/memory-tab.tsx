'use client';

import { Brain, Eye, Save, Search, Trash2, X } from 'lucide-react';
import { EmptyState, StatusBadge, SubsectionTitle } from './panel-components';
import { MEMORY_TYPE_OPTIONS } from './panel-types';
import type { MemoryItem } from './panel-types';
import { memoryEntryKey, memoryResultsSummaryLabel, memoryScopeLabel, memoryTimestampLabel } from './memory-helpers';
import { useMemoryPanel } from './use-memory';

type MemoryListRowProps = {
  entry: MemoryItem;
  busy: boolean;
  onView: (memoryId: string) => void;
  onDelete: (memoryId: string) => void;
};

export function MemoryListRow({ entry, busy, onView, onDelete }: MemoryListRowProps) {
  return (
    <div className={`job-row watch-row ${entry.status}`}>
      <div className="job-row-main">
        <strong title={entry.id}>{entry.title || entry.id}</strong>
        <small className="job-row-meta">
          <span>{entry.memory_type}</span>
          <span>{memoryScopeLabel(entry)}</span>
          {typeof entry.score === 'number' && <span>score {entry.score.toFixed(2)}</span>}
        </small>
      </div>
      <div className="watch-row-actions">
        <StatusBadge status={entry.status} />
        <button className="ghost icon-button" title="View memory" disabled={busy} onClick={() => onView(entry.id)}>
          <Eye aria-hidden="true" className="button-icon" />
        </button>
        <button className="ghost icon-button danger" title="Delete memory" disabled={busy} onClick={() => onDelete(entry.id)}>
          <Trash2 aria-hidden="true" className="button-icon" />
        </button>
      </div>
    </div>
  );
}

export function MemoryTab() {
  const {
    searchQuery, setSearchQuery,
    searchResults,
    searchLoading,
    searchMessage,
    runSearch,
    selectedMemoryId,
    selectedMemory,
    detailLoading,
    detailMessage,
    deleteBusyId,
    viewMemory,
    closeMemoryDetail,
    deleteMemory,
    rememberType, setRememberType,
    rememberTitle, setRememberTitle,
    rememberBody, setRememberBody,
    rememberProject, setRememberProject,
    rememberRepo, setRememberRepo,
    rememberFile, setRememberFile,
    rememberConfidence, setRememberConfidence,
    rememberBusy,
    rememberMessage,
    rememberResult,
    submitRemember
  } = useMemoryPanel();

  return (
    <section className="stack-panel">
      <div className="section-heading">
        <div className="health-title">
          <div className={`status-orb ${searchMessage ? 'error' : searchResults ? 'ok' : 'warn'}`}>
            <Brain aria-hidden="true" className="status-glyph" />
          </div>
          <div>
            <p className="eyebrow">Memory</p>
            <h2>Agent Memory</h2>
            <p>{memoryResultsSummaryLabel(searchResults, searchQuery)}</p>
          </div>
        </div>
      </div>

      <div className="status-grid">
        <div className="status-panel">
          <SubsectionTitle icon={Search} title="Search Memories" note="POST /v1/memories/search — semantic recall over durable memory." />
          <label>
            Query
            <input
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') void runSearch();
              }}
              placeholder="leave blank to list recent memories"
            />
          </label>
          <button onClick={() => void runSearch()} disabled={searchLoading}>
            <Search aria-hidden="true" className={`button-icon ${searchLoading ? 'spin' : ''}`} />
            {searchLoading ? 'Searching' : 'Search'}
          </button>
          {searchMessage && <p className="error">{searchMessage}</p>}
          {searchResults && searchResults.length ? (
            <div className="job-list">
              {searchResults.map((entry, index) => (
                <MemoryListRow
                  key={memoryEntryKey(entry, index)}
                  entry={entry}
                  busy={deleteBusyId === entry.id}
                  onView={viewMemory}
                  onDelete={deleteMemory}
                />
              ))}
            </div>
          ) : (
            <EmptyState loading={searchLoading} text="No memories found." />
          )}
        </div>

        <div className="status-panel command-card">
          <SubsectionTitle icon={Save} title="Remember" note="POST /v1/memories — store a new durable memory." />
          <label>
            Type
            <select value={rememberType} onChange={(event) => setRememberType(event.target.value as typeof rememberType)}>
              {MEMORY_TYPE_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Title (optional)
            <input value={rememberTitle} onChange={(event) => setRememberTitle(event.target.value)} placeholder="derived from body if blank" />
          </label>
          <label>
            Body
            <textarea value={rememberBody} onChange={(event) => setRememberBody(event.target.value)} rows={4} placeholder="What should Axon remember?" />
          </label>
          <label>
            Project (optional)
            <input value={rememberProject} onChange={(event) => setRememberProject(event.target.value)} />
          </label>
          <label>
            Repo (optional)
            <input value={rememberRepo} onChange={(event) => setRememberRepo(event.target.value)} />
          </label>
          <label>
            File (optional)
            <input value={rememberFile} onChange={(event) => setRememberFile(event.target.value)} />
          </label>
          <label>
            Confidence 0-1 (optional)
            <input value={rememberConfidence} onChange={(event) => setRememberConfidence(event.target.value)} placeholder="1.0" />
          </label>
          <button onClick={() => void submitRemember()} disabled={rememberBusy || !rememberBody.trim()}>
            <Save aria-hidden="true" className="button-icon" />
            {rememberBusy ? 'Saving' : 'Remember'}
          </button>
          {rememberMessage && <p className="error">{rememberMessage}</p>}
          {rememberResult && (
            <p className="empty-state">Saved memory {rememberResult.id} ({rememberResult.status}).</p>
          )}
        </div>
      </div>

      {selectedMemoryId && (
        <div className="status-panel">
          <div className="section-heading">
            <SubsectionTitle icon={Eye} title="Memory Detail" note={`GET /v1/memories/${selectedMemoryId}`} />
            <button className="ghost icon-button" title="Close detail" onClick={closeMemoryDetail}>
              <X aria-hidden="true" className="button-icon" />
            </button>
          </div>
          {detailLoading && <EmptyState loading text="Loading memory..." />}
          {detailMessage && <p className="error">{detailMessage}</p>}
          {selectedMemory && (
            <div className="job-row-main">
              <strong>{selectedMemory.title || selectedMemory.id}</strong>
              <p>{selectedMemory.body}</p>
              <small className="job-row-meta">
                <span>{selectedMemory.memory_type}</span>
                <span>{selectedMemory.status}</span>
                <span>{memoryScopeLabel(selectedMemory)}</span>
                <span>confidence {selectedMemory.confidence}</span>
                <span>updated {memoryTimestampLabel(selectedMemory.updated_at)}</span>
              </small>
              <button
                className="ghost icon-button danger"
                title="Delete memory"
                disabled={deleteBusyId === selectedMemory.id}
                onClick={() => void deleteMemory(selectedMemory.id)}
              >
                <Trash2 aria-hidden="true" className="button-icon" />
                Delete
              </button>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
