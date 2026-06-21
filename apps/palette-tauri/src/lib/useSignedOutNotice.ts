import { useCallback, useEffect, useState } from "react";

import { appWindow } from "@/lib/invoke";
import { oauthStatus } from "@/lib/oauthClient";

export const SIGNED_OUT_NOTICE =
  "You've been signed out of Axon — sign in again in Settings.";

/// App-wide "signed out" banner state. A reactive 401 on any action path
/// (ask/query/…) clears a dead OAuth session in the Rust shell and emits
/// `palette://oauth-changed`. `SettingsAuthBlock` only re-syncs while Settings is
/// open, so this hook listens for the same event app-wide: when it fires it
/// re-checks `oauthStatus()` and surfaces a dismissible notice if the session is
/// no longer signed in (signed out, or a credential for a different server) — and
/// clears the notice if the session is signed in again. Lives in `src/lib` per
/// the repo architecture rule (stateful side effects out of components);
/// `AuthNotice` is a thin renderer over `{ notice, dismiss }`. The browser-dev
/// `appWindow.listen` is a no-op stub, so this stays inert (and harmless) under
/// `pnpm vite:dev` and in tests.
export interface SignedOutNotice {
  notice: string | null;
  dismiss: () => void;
}

export function useSignedOutNotice(): SignedOutNotice {
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    const unlisten = appWindow.listen("palette://oauth-changed", () => {
      void (async () => {
        try {
          const status = await oauthStatus();
          if (!active) return;
          // Surface the notice while signed out; clear it once signed in again.
          setNotice(status.signedIn ? null : SIGNED_OUT_NOTICE);
        } catch {
          // A failed status read is not itself a sign-out signal; stay quiet.
        }
      })();
    });
    return () => {
      active = false;
      void unlisten.then((u) => u());
    };
  }, []);

  const dismiss = useCallback(() => setNotice(null), []);

  return { notice, dismiss };
}
