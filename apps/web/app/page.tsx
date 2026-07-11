'use client';

import { Command, LockKeyhole } from 'lucide-react';

import { commandExamples } from '../src/lib/command-format';
import { CommandPalette } from '../src/lib/command-palette';
import { PanelNav } from '../src/lib/panel-nav';
import { TOKEN_KEY } from '../src/lib/panel-types';
import { usePanelData } from '../src/lib/use-panel-data';
import { LoginPanel } from '../src/auth/login-panel';
import { DashboardTab } from '../src/features/dashboard/DashboardTab';
import { JobsTab } from '../src/features/jobs/JobsTab';
import { SourcesTab } from '../src/features/sources/SourcesTab';
import { WatchesTab } from '../src/features/watches/watches-tab';
import { MemoryTab } from '../src/features/memory/memory-tab';
import { ConfiguratorTab } from '../src/features/config/ConfiguratorTab';

export default function Page() {
  const {
    token, setToken,
    password, setPassword,
    message,
    login,
    panelState,
    activePanelTab, setActivePanelTab,
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
    axonStatus,
    liveJobs,
    activeJobs,
    statusMessage,
    statusUpdatedAt,
    activeConfigFile, setActiveConfigFile,
    activeConfigPath,
    activeConfigMeta,
    activeConfigValue,
    activeDirty,
    configDirty,
    envDirty,
    updateActiveConfig,
    revertConfig,
    saveConfig,
    paletteOpen, setPaletteOpen,
    commandInput, setCommandInput,
    commandBusy,
    commandResult,
    commandHistory,
    runCommand,
    refreshDashboard,
    refreshAxonStatus,
    sourceEntries,
    sourcesResult,
    sourcesLoading,
    sourcesMessage,
    sourcesUpdatedAt,
    refreshSources,
    sourceFormValue, setSourceFormValue,
    sourceFormFamily, setSourceFormFamily,
    sourceFormEmbed, setSourceFormEmbed,
    sourceFormMaxPages, setSourceFormMaxPages,
    sourceSubmitBusy,
    sourceSubmitResult,
    sourceSubmitError,
    submitSourceForm,
  } = usePanelData();

  if (!token) {
    return (
      <LoginPanel panelState={panelState} password={password} setPassword={setPassword} login={login} message={message} />
    );
  }

  return (
    <main className="shell">
      <header className="topbar">
        <div className="brand-heading">
          <img className="brand-mark" src="/assets/axon-glyph.svg" alt="" aria-hidden="true" />
          <div>
            <p className="eyebrow">Axon Admin</p>
            <h1>{panelState?.setup_required ? 'Setup Wizard' : 'Management Dashboard'}</h1>
          </div>
        </div>
        <div className="topbar-actions">
          <button className="command-launch" onClick={() => setPaletteOpen(true)}>
            <Command aria-hidden="true" className="button-icon" />
            Command
            <kbd>⌘K</kbd>
          </button>
          <button
            className="ghost"
            onClick={() => {
              window.sessionStorage.removeItem(TOKEN_KEY);
              setToken('');
            }}
          >
            <LockKeyhole aria-hidden="true" className="button-icon" />
            Lock
          </button>
        </div>
      </header>

      <PanelNav
        activePanelTab={activePanelTab}
        setActivePanelTab={setActivePanelTab}
        configDirty={configDirty}
        envDirty={envDirty}
        activeJobCount={activeJobs.length}
      />

      {activePanelTab === 'dashboard' && (
        <DashboardTab
          stack={stack}
          stackLoading={stackLoading}
          stackStatus={stackStatus}
          stackUpdatedAt={stackUpdatedAt}
          doctor={doctor}
          doctorMessage={doctorMessage}
          doctorUpdatedAt={doctorUpdatedAt}
          urlSummary={urlSummary}
          runtimeChecks={runtimeChecks}
          skippedHostChecks={skippedHostChecks}
          overallStatus={overallStatus}
          doctorServices={doctorServices}
          doctorSummary={doctorSummary}
          refreshDashboard={refreshDashboard}
        />
      )}

      {activePanelTab === 'jobs' && (
        <JobsTab
          activeJobs={activeJobs}
          liveJobs={liveJobs}
          axonStatus={axonStatus}
          statusMessage={statusMessage}
          statusUpdatedAt={statusUpdatedAt}
          stackLoading={stackLoading}
          refreshAxonStatus={refreshAxonStatus}
          setPaletteOpen={setPaletteOpen}
          setCommandInput={setCommandInput}
        />
      )}

      {activePanelTab === 'sources' && (
        <SourcesTab
          sourceEntries={sourceEntries}
          sourcesResult={sourcesResult}
          sourcesLoading={sourcesLoading}
          sourcesMessage={sourcesMessage}
          sourcesUpdatedAt={sourcesUpdatedAt}
          refreshSources={refreshSources}
          sourceFormValue={sourceFormValue}
          setSourceFormValue={setSourceFormValue}
          sourceFormFamily={sourceFormFamily}
          setSourceFormFamily={setSourceFormFamily}
          sourceFormEmbed={sourceFormEmbed}
          setSourceFormEmbed={setSourceFormEmbed}
          sourceFormMaxPages={sourceFormMaxPages}
          setSourceFormMaxPages={setSourceFormMaxPages}
          sourceSubmitBusy={sourceSubmitBusy}
          sourceSubmitResult={sourceSubmitResult}
          sourceSubmitError={sourceSubmitError}
          submitSourceForm={submitSourceForm}
        />
      )}

      {activePanelTab === 'watches' && <WatchesTab token={token} active={activePanelTab === 'watches'} />}

      {activePanelTab === 'memory' && <MemoryTab />}

      {activePanelTab === 'configurator' && (
        <ConfiguratorTab
          activeConfigFile={activeConfigFile}
          setActiveConfigFile={setActiveConfigFile}
          activeConfigPath={activeConfigPath}
          activeConfigMeta={activeConfigMeta}
          activeConfigValue={activeConfigValue}
          activeDirty={activeDirty}
          configDirty={configDirty}
          envDirty={envDirty}
          updateActiveConfig={updateActiveConfig}
          revertConfig={revertConfig}
          saveConfig={saveConfig}
          message={message}
        />
      )}

      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        commandInput={commandInput}
        setCommandInput={setCommandInput}
        commandHistory={commandHistory}
        commandExamples={commandExamples}
        runCommand={runCommand}
        commandBusy={commandBusy}
        commandResult={commandResult}
        panelToken={token}
      />
    </main>
  );
}
