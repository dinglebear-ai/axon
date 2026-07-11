'use client';

import { useMemo, useState } from 'react';
import { AxonClient } from '../../api/axon-client';
import type { MemoryItem, MemoryNodeType } from '../../lib/panel-types';
import { memoryErrorMessage, parseConfidence } from './memory-helpers';

/**
 * Standalone hook for the Memory tab (POST /v1/memories, POST
 * /v1/memories/search, GET/DELETE /v1/memories/{id}). Kept separate from
 * usePanelData()/use-panel-data.ts — that file is already at the monolith
 * line-count limit from the Sources tab (a sibling workstream) — so memory
 * state and actions are self-contained here and consumed only by
 * memory-tab.tsx, mirroring use-watches.ts.
 */
export function useMemoryPanel() {
  const axonClient = useMemo(() => new AxonClient(), []);

  // Search
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<MemoryItem[] | null>(null);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchMessage, setSearchMessage] = useState('');

  // Detail
  const [selectedMemoryId, setSelectedMemoryId] = useState<string | null>(null);
  const [selectedMemory, setSelectedMemory] = useState<MemoryItem | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailMessage, setDetailMessage] = useState('');
  const [deleteBusyId, setDeleteBusyId] = useState<string | null>(null);

  // Remember form
  const [rememberType, setRememberType] = useState<MemoryNodeType>('fact');
  const [rememberTitle, setRememberTitle] = useState('');
  const [rememberBody, setRememberBody] = useState('');
  const [rememberProject, setRememberProject] = useState('');
  const [rememberRepo, setRememberRepo] = useState('');
  const [rememberFile, setRememberFile] = useState('');
  const [rememberConfidence, setRememberConfidence] = useState('');
  const [rememberBusy, setRememberBusy] = useState(false);
  const [rememberMessage, setRememberMessage] = useState('');
  const [rememberResult, setRememberResult] = useState<MemoryItem | null>(null);

  async function runSearch() {
    setSearchLoading(true);
    setSearchMessage('');
    try {
      const result = await axonClient.searchMemories({ query: searchQuery.trim(), limit: 50 });
      setSearchResults(result.memories ?? []);
    } catch (error) {
      setSearchResults(null);
      setSearchMessage(memoryErrorMessage(error));
    } finally {
      setSearchLoading(false);
    }
  }

  async function viewMemory(memoryId: string) {
    setSelectedMemoryId(memoryId);
    setSelectedMemory(null);
    setDetailLoading(true);
    setDetailMessage('');
    try {
      const result = await axonClient.showMemory(memoryId);
      setSelectedMemory(result.memory ?? null);
      if (!result.memory) setDetailMessage('Memory not found.');
    } catch (error) {
      setDetailMessage(memoryErrorMessage(error));
    } finally {
      setDetailLoading(false);
    }
  }

  function closeMemoryDetail() {
    setSelectedMemoryId(null);
    setSelectedMemory(null);
    setDetailMessage('');
  }

  async function deleteMemory(memoryId: string) {
    if (typeof window !== 'undefined' && !window.confirm(`Delete memory ${memoryId}? This cannot be undone.`)) {
      return;
    }
    setDeleteBusyId(memoryId);
    setDetailMessage('');
    try {
      await axonClient.deleteMemory(memoryId);
      if (selectedMemoryId === memoryId) closeMemoryDetail();
      setSearchResults((current) => (current ? current.filter((item) => item.id !== memoryId) : current));
    } catch (error) {
      setDetailMessage(memoryErrorMessage(error));
    } finally {
      setDeleteBusyId(null);
    }
  }

  function resetRememberForm() {
    setRememberType('fact');
    setRememberTitle('');
    setRememberBody('');
    setRememberProject('');
    setRememberRepo('');
    setRememberFile('');
    setRememberConfidence('');
  }

  async function submitRemember() {
    if (!rememberBody.trim()) {
      setRememberMessage('Body is required.');
      return;
    }
    const confidence = parseConfidence(rememberConfidence);
    if (rememberConfidence.trim() && confidence === null) {
      setRememberMessage('Confidence must be a number between 0 and 1.');
      return;
    }
    setRememberBusy(true);
    setRememberMessage('');
    try {
      const result = await axonClient.rememberMemory({
        memory_type: rememberType,
        title: rememberTitle.trim() || null,
        body: rememberBody.trim(),
        project: rememberProject.trim() || null,
        repo: rememberRepo.trim() || null,
        file: rememberFile.trim() || null,
        confidence: confidence ?? undefined
      });
      setRememberResult(result.memory ?? null);
      resetRememberForm();
    } catch (error) {
      setRememberMessage(memoryErrorMessage(error));
    } finally {
      setRememberBusy(false);
    }
  }

  return {
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
  };
}
