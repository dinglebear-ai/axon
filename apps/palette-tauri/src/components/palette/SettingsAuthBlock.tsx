import { KeyRound } from "lucide-react";

import { Button } from "@/components/ui/aurora/button";
import { useOauthSession } from "@/lib/useOauthSession";

/// OAuth authentication panel for the Settings connection tab. Thin renderer
/// over `useOauthSession()`, which owns sign-in status (re-fetched on mount and
/// on `palette://oauth-changed`), the `busy`/`error` state, and the
/// `signIn`/`signOut` actions.
export function SettingsAuthBlock() {
  const { busy, error, view, signIn, signOut } = useOauthSession();

  return (
    <div className="settings-stack">
      <span className="settings-section-label">Authentication</span>
      <div className="settings-auth-status" data-tone={view.tone} aria-live="polite">
        <strong>{view.label}</strong>
        <span>{view.detail}</span>
        {error && <span className="settings-error">{error}</span>}
      </div>
      {view.tone === "success" ? (
        <Button size="sm" variant="neutral" disabled={busy} onClick={() => void signOut()}>
          <KeyRound size={14} />
          {busy ? "Working…" : "Sign out"}
        </Button>
      ) : (
        <Button size="sm" variant="aurora" disabled={busy} onClick={() => void signIn()}>
          <KeyRound size={14} />
          {busy ? "Opening browser…" : "Sign in with Google"}
        </Button>
      )}
    </div>
  );
}
