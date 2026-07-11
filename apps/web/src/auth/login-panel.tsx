'use client';

import { LockKeyhole } from 'lucide-react';

import type { PanelState } from '../lib/panel-types';

export function LoginPanel({
  panelState,
  password,
  setPassword,
  login,
  message,
}: {
  panelState: PanelState | null;
  password: string;
  setPassword: (value: string) => void;
  login: () => Promise<void> | void;
  message: string;
}) {
  return (
    <main className="shell narrow">
      <section className="login-panel">
        <div className="brand-heading">
          <img className="brand-mark" src="/assets/axon-glyph.svg" alt="" aria-hidden="true" />
          <div>
            <p className="eyebrow">Axon Admin</p>
            <h1>{panelState?.setup_required ? 'Setup Wizard' : 'Management Dashboard'}</h1>
            <p className="muted">{panelState?.config_path ?? '~/.axon/config.toml'}</p>
          </div>
        </div>
        <label>
          Panel password
          <input
            type="password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') void login();
            }}
            autoFocus
          />
        </label>
        <button onClick={() => void login()}>
          <LockKeyhole aria-hidden="true" className="button-icon" />
          Unlock
        </button>
        {message && <p className="error">{message}</p>}
      </section>
    </main>
  );
}
