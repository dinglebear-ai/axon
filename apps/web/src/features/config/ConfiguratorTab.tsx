'use client';

import { useState } from 'react';
import { Braces, ClipboardCopy, FileCog, RotateCcw, Save, ShieldCheck, Trash2 } from 'lucide-react';

import { savedMessage } from '../jobs/job-helpers';
import type { ConfigFile, EnvConfigKeyState } from '../../lib/panel-types';

export function ConfiguratorTab({
  activeConfigFile,
  setActiveConfigFile,
  activeConfigPath,
  activeConfigMeta,
  activeConfigValue,
  activeDirty,
  configDirty,
  envDirty,
  envKeys,
  envSaveBusy,
  updateActiveConfig,
  revertConfig,
  saveConfig,
  saveEnvKey,
  message
}: {
  activeConfigFile: ConfigFile;
  setActiveConfigFile: (file: ConfigFile) => void;
  activeConfigPath: string | undefined;
  activeConfigMeta: { lines: number; characters: number };
  activeConfigValue: string;
  activeDirty: boolean;
  configDirty: boolean;
  envDirty: boolean;
  envKeys: EnvConfigKeyState[];
  envSaveBusy: string | null;
  updateActiveConfig: (value: string) => void;
  revertConfig: () => void;
  saveConfig: () => Promise<void>;
  saveEnvKey: (key: string, value: string | null) => Promise<boolean>;
  message: string;
}) {
  const [envDrafts, setEnvDrafts] = useState<Record<string, string>>({});
  const configuredCount = envKeys.filter((entry) => entry.configured).length;

  function updateEnvDraft(key: string, value: string) {
    setEnvDrafts((drafts) => ({ ...drafts, [key]: value }));
  }

  function clearEnvDraft(key: string) {
    setEnvDrafts((drafts) => {
      const next = { ...drafts };
      delete next[key];
      return next;
    });
  }

  async function applyEnvDraft(key: string) {
    if (!(key in envDrafts)) return;
    if (await saveEnvKey(key, envDrafts[key] ?? '')) clearEnvDraft(key);
  }

  async function clearEnvValue(key: string) {
    if (await saveEnvKey(key, null)) clearEnvDraft(key);
  }

  return (
    <section className="workbench-shell">
      <div className="workbench-header">
        <div className="section-heading">
          <div>
            <h2>
              <FileCog aria-hidden="true" className="heading-icon" />
              Configurator
            </h2>
            <p>Manage config.toml and the panel-safe runtime environment surface.</p>
          </div>
        </div>
        <span className="workbench-path">config.toml + safe env inventory</span>
      </div>

      <div className="editor-panel">
        <div className="editor-toolbar">
          <div>
            <h2>
              <FileCog aria-hidden="true" className="heading-icon" />
              {activeConfigFile === 'toml' ? 'config.toml' : 'Environment'}
            </h2>
            <p>
              {activeConfigFile === 'toml'
                ? activeConfigPath
                : 'Current values are never returned to the browser. Only allowlisted key names and configured state are visible.'}
            </p>
          </div>
          {activeConfigFile === 'toml' && (
            <div className="editor-actions">
              <button
                className="ghost"
                onClick={() => void navigator.clipboard?.writeText(activeConfigPath ?? '')}
                disabled={!activeConfigPath}
                title="Copy active config path"
              >
                <ClipboardCopy aria-hidden="true" className="button-icon" />
                Copy path
              </button>
              <button className="ghost" onClick={revertConfig} disabled={!activeDirty}>
                <RotateCcw aria-hidden="true" className="button-icon" />
                Revert
              </button>
              <button onClick={() => void saveConfig()} disabled={!activeDirty}>
                <Save aria-hidden="true" className="button-icon" />
                Save
              </button>
            </div>
          )}
        </div>

        <div className="config-tabs" role="tablist" aria-label="Config surface">
          <button
            className={activeConfigFile === 'toml' ? 'selected' : ''}
            onClick={() => setActiveConfigFile('toml')}
            role="tab"
            aria-selected={activeConfigFile === 'toml'}
          >
            <FileCog aria-hidden="true" className="button-icon" />
            config.toml
            {configDirty && <span className="dirty-dot" aria-label="Modified" />}
          </button>
          <button
            className={activeConfigFile === 'env' ? 'selected' : ''}
            onClick={() => setActiveConfigFile('env')}
            role="tab"
            aria-selected={activeConfigFile === 'env'}
          >
            <ShieldCheck aria-hidden="true" className="button-icon" />
            Environment
            {envDirty && <span className="dirty-dot" aria-label="Modified" />}
          </button>
        </div>

        {activeConfigFile === 'toml' ? (
          <>
            <div className="editor-meta" aria-label="Config metadata">
              <span>
                <Braces aria-hidden="true" className="inline-icon" />
                {activeConfigMeta.lines} lines
              </span>
              <span>{activeConfigMeta.characters} chars</span>
              <span>TOML validated on save</span>
              <span className={activeDirty ? 'meta-dirty' : ''}>{activeDirty ? 'Modified' : 'Saved'}</span>
            </div>
            <textarea
              value={activeConfigValue}
              onChange={(event) => updateActiveConfig(event.target.value)}
              spellCheck={false}
            />
          </>
        ) : (
          <div className="env-inventory">
            <div className="editor-meta" aria-label="Environment metadata">
              <span>
                <ShieldCheck aria-hidden="true" className="inline-icon" />
                {configuredCount} configured
              </span>
              <span>{envKeys.length} allowlisted keys</span>
              <span>Values hidden by policy</span>
              <span>Restart after changes</span>
            </div>
            <div className="env-key-list">
              {envKeys.map((entry) => {
                const touched = Object.prototype.hasOwnProperty.call(envDrafts, entry.key);
                const busy = envSaveBusy === entry.key;
                return (
                  <div className="env-key-row" key={entry.key}>
                    <div className="env-key-identity">
                      <code>{entry.key}</code>
                      <span className={entry.configured ? 'env-state configured' : 'env-state'}>
                        {entry.configured ? 'Configured' : 'Not configured'}
                      </span>
                    </div>
                    <input
                      aria-label={`Replacement value for ${entry.key}`}
                      autoComplete="off"
                      spellCheck={false}
                      value={envDrafts[entry.key] ?? ''}
                      onChange={(event) => updateEnvDraft(entry.key, event.target.value)}
                      placeholder="Enter replacement value"
                    />
                    <div className="env-key-actions">
                      <button
                        onClick={() => void applyEnvDraft(entry.key)}
                        disabled={!touched || busy}
                      >
                        <Save aria-hidden="true" className="button-icon" />
                        {busy ? 'Saving' : 'Set'}
                      </button>
                      <button
                        className="ghost"
                        onClick={() => void clearEnvValue(entry.key)}
                        disabled={!entry.configured || busy}
                      >
                        <Trash2 aria-hidden="true" className="button-icon" />
                        Clear
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {message && <p className={savedMessage(message) ? 'ok' : 'error'}>{message}</p>}
      </div>
    </section>
  );
}
