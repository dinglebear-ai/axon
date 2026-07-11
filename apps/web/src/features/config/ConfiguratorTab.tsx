'use client';

import { Braces, ClipboardCopy, FileCog, RotateCcw, Save } from 'lucide-react';

import { savedMessage } from '../jobs/job-helpers';
import type { ConfigFile } from '../../lib/panel-types';

export function ConfiguratorTab({
  activeConfigFile,
  setActiveConfigFile,
  activeConfigPath,
  activeConfigMeta,
  activeConfigValue,
  activeDirty,
  configDirty,
  envDirty,
  updateActiveConfig,
  revertConfig,
  saveConfig,
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
  updateActiveConfig: (value: string) => void;
  revertConfig: () => void;
  saveConfig: () => Promise<void>;
  message: string;
}) {
  return (
    <section className="workbench-shell">
      <div className="workbench-header">
        <div className="section-heading">
          <div>
            <h2>
              <FileCog aria-hidden="true" className="heading-icon" />
              Configurator
            </h2>
            <p>Manage `config.toml` and `.env` without leaving the dashboard.</p>
          </div>
        </div>
        <span className="workbench-path">config.toml and .env</span>
      </div>

      <div className="editor-panel">
        <div className="editor-toolbar">
          <div>
            <h2>
              <FileCog aria-hidden="true" className="heading-icon" />
              Configurator
            </h2>
            <p>{activeConfigPath}</p>
          </div>
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
        </div>
        <div className="config-tabs" role="tablist" aria-label="Config file">
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
            <Braces aria-hidden="true" className="button-icon" />
            .env
            {envDirty && <span className="dirty-dot" aria-label="Modified" />}
          </button>
        </div>
        <div className="editor-meta" aria-label="Config metadata">
          <span>
            <Braces aria-hidden="true" className="inline-icon" />
            {activeConfigMeta.lines} lines
          </span>
          <span>{activeConfigMeta.characters} chars</span>
          <span>{activeConfigFile === 'toml' ? 'TOML validated on save' : 'dotenv parsed on save'}</span>
          <span className={activeDirty ? 'meta-dirty' : ''}>{activeDirty ? 'Modified' : 'Saved'}</span>
        </div>
        <textarea
          value={activeConfigValue}
          onChange={(event) => updateActiveConfig(event.target.value)}
          spellCheck={false}
        />
        {message && <p className={savedMessage(message) ? 'ok' : 'error'}>{message}</p>}
      </div>
    </section>
  );
}
