import { useCallback, useEffect, useState } from "react";

import { appWindow } from "@/lib/invoke";
import {
  describeOauthStatus,
  oauthLogin,
  oauthLogout,
  oauthStatus,
  type OauthStatus,
} from "@/lib/oauthClient";

/// Stateful OAuth session hook for the Settings connection tab. Owns the
/// `{ status, busy, error }` triple plus the mount-fetch and the
/// `palette://oauth-changed` listener (a reactive 401 refresh in the Rust shell
/// emits that event after clearing a dead session), re-fetching status whenever
/// it fires so the UI stays in sync without a manual reload. Lives in `src/lib`
/// per the repo architecture rule (stateful side effects out of components);
/// `SettingsAuthBlock` is a thin renderer over what this returns. The browser-dev
/// `appWindow.listen` is a no-op stub, so the listener stays inert (and harmless)
/// under `pnpm vite:dev` and in tests.
export interface OauthSession {
  status: OauthStatus | null;
  busy: boolean;
  error: string | null;
  view: { label: string; detail: string; tone: "neutral" | "success" | "error" };
  signIn: () => Promise<void>;
  signOut: () => Promise<void>;
}

const CHECKING_VIEW = {
  label: "Checking…",
  detail: "Reading saved credentials…",
  tone: "neutral" as const,
};

export function useOauthSession(): OauthSession {
  const [status, setStatus] = useState<OauthStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;

    const load = async () => {
      try {
        const next = await oauthStatus();
        if (!active) return;
        setStatus(next);
        // A successful read supersedes any stale error from a prior failure.
        setError(null);
      } catch (err) {
        if (!active) return;
        setStatus({ signedIn: false, scope: null, expiresAtUnix: null, serverUrl: null });
        setError(err instanceof Error ? err.message : "Could not read sign-in status.");
      }
    };

    void load();
    const unlisten = appWindow.listen("palette://oauth-changed", () => {
      void load();
    });
    return () => {
      active = false;
      void unlisten.then((u) => u());
    };
  }, []);

  // Run an action that returns a fresh status (sign-in/sign-out), reflecting it
  // into state while toggling `busy` and clearing/setting `error`.
  const run = useCallback(async (action: () => Promise<OauthStatus>) => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await action());
    } catch (err) {
      setError(err instanceof Error ? err.message : "OAuth request failed.");
    } finally {
      setBusy(false);
    }
  }, []);

  const signIn = useCallback(() => run(oauthLogin), [run]);
  const signOut = useCallback(() => run(oauthLogout), [run]);

  const view = status ? describeOauthStatus(status) : CHECKING_VIEW;

  return { status, busy, error, view, signIn, signOut };
}
