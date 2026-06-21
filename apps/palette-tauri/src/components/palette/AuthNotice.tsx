import { X } from "lucide-react";

import { Button } from "@/components/ui/aurora/button";
import { useSignedOutNotice } from "@/lib/useSignedOutNotice";

/// App-wide "signed out" banner. Thin renderer over `useSignedOutNotice()`,
/// which owns the notice state and the `palette://oauth-changed` listener (see
/// that hook for the reactive-401 rationale). Renders nothing until the hook
/// surfaces a notice.
export function AuthNotice() {
  const { notice, dismiss } = useSignedOutNotice();

  if (!notice) return null;

  return (
    <div className="palette-auth-notice" role="status" aria-live="polite">
      <span>{notice}</span>
      <Button
        variant="plain"
        size="unstyled"
        type="button"
        className="palette-auth-notice-dismiss"
        aria-label="Dismiss"
        onClick={dismiss}
      >
        <X size={14} />
      </Button>
    </div>
  );
}
