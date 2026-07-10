'use client';

import { Command, Play, Terminal, X } from 'lucide-react';

import { CommandResultCard } from './panel-components';
import type { CommandResultView } from './panel-types';

export function CommandPalette({
  open,
  onClose,
  commandInput,
  setCommandInput,
  commandHistory,
  commandExamples,
  runCommand,
  commandBusy,
  commandResult,
  panelToken,
}: {
  open: boolean;
  onClose: () => void;
  commandInput: string;
  setCommandInput: (value: string) => void;
  commandHistory: string[];
  commandExamples: string[];
  runCommand: (command?: string) => Promise<void> | void;
  commandBusy: boolean;
  commandResult: CommandResultView | null;
  panelToken: string;
}) {
  if (!open) return null;

  return (
    <div className="palette-backdrop" role="presentation" onMouseDown={onClose}>
      <section className="command-palette" role="dialog" aria-modal="true" onMouseDown={(event) => event.stopPropagation()}>
        <div className="palette-input-row">
          <Command aria-hidden="true" className="heading-icon" />
          <input
            value={commandInput}
            onChange={(event) => setCommandInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') void runCommand();
            }}
            placeholder="scrape code.claude.com"
            autoFocus
          />
          <button className="ghost icon-button" onClick={onClose} title="Close palette">
            <X aria-hidden="true" className="button-icon" />
          </button>
        </div>
        <div className="palette-body">
          <div className="palette-suggestions">
            {[...commandHistory, ...commandExamples]
              .filter((item, index, all) => all.indexOf(item) === index)
              .slice(0, 8)
              .map((example) => (
                <button
                  className="palette-suggestion"
                  key={example}
                  onClick={() => {
                    setCommandInput(example);
                    void runCommand(example);
                  }}
                >
                  <Terminal aria-hidden="true" className="button-icon" />
                  <span>{example}</span>
                  <Play aria-hidden="true" className="inline-icon" />
                </button>
              ))}
          </div>
          <button className="command-run" onClick={() => void runCommand()} disabled={commandBusy || !commandInput.trim()}>
            <Play aria-hidden="true" className="button-icon" />
            {commandBusy ? 'Running' : 'Run command'}
          </button>
          {commandResult && <CommandResultCard result={commandResult} panelToken={panelToken} />}
        </div>
      </section>
    </div>
  );
}
