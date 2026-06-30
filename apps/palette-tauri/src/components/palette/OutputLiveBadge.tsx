import { Play, RotateCw } from "lucide-react";

import { Button } from "@/components/ui/aurora/button";
import type { LiveRefreshState } from "@/lib/useLiveRefresh";

export function OutputLiveBadge({
  state,
  onTogglePause,
  onRefreshNow,
}: {
  state: LiveRefreshState;
  onTogglePause?: () => void;
  onRefreshNow: () => void;
}) {
  const ago = state.lastRefreshedAtMs
    ? `${Math.max(0, Math.round((Date.now() - state.lastRefreshedAtMs) / 1000))}s ago`
    : "live";
  return (
    <span className={state.paused ? "output-live output-live-paused" : "output-live"}>
      <Button
        variant="plain"
        size="unstyled"
        type="button"
        className="output-live-toggle"
        onClick={onTogglePause}
        title={state.paused ? "Resume auto-refresh" : "Pause auto-refresh"}
        aria-label={state.paused ? "Resume auto-refresh" : "Pause auto-refresh"}
      >
        {state.paused ? <Play size={11} /> : <span className="output-live-dot" aria-hidden="true" />}
        {state.paused ? "PAUSED" : "LIVE"}
      </Button>
      <span className="output-live-ago">{state.paused ? "" : `· ${ago}`}</span>
      <Button
        variant="plain"
        size="unstyled"
        type="button"
        className="output-live-refresh"
        onClick={onRefreshNow}
        title="Refresh now"
        aria-label="Refresh now"
      >
        <RotateCw size={12} />
      </Button>
    </span>
  );
}
