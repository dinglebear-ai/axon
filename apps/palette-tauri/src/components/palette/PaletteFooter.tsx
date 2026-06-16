import { Settings, X } from "lucide-react";

import { StatusIndicator } from "@/components/ui/aurora/status-indicator";
import type { PaletteConfig } from "@/lib/axonClient";
import { hostLabel } from "@/lib/url";

interface PaletteFooterProps {
  config: PaletteConfig | null;
  configError: string | null;
  onRecent: () => void;
  onSettings: () => void;
  onHide: () => void;
}

// Footer row: keyboard hint legend on the left, endpoint status + settings/hide
// controls on the right.
export function PaletteFooter({ config, configError, onRecent, onSettings, onHide }: PaletteFooterProps) {
  return (
    <footer className="palette-footer">
      <span className="palette-footer-hints">
        <button className="palette-recent" type="button" onClick={onRecent}>↺ recent</button>
        <span className="palette-hint-group"><kbd>↑</kbd><kbd>↓</kbd> navigate</span>
        <span className="palette-hint-group"><kbd>tab</kbd> select</span>
        <span className="palette-hint-group"><kbd>↵</kbd> run</span>
        <span className="palette-hint-group"><kbd>esc</kbd> close</span>
      </span>
      <span className="palette-status" role="group" aria-label="Palette controls">
        {config ? (
          <StatusIndicator tone="syncing" label={`${hostLabel(config.serverUrl)} / ${config.collection}`} pulse={false} />
        ) : configError ? (
          <StatusIndicator tone="error" label="Config error" />
        ) : (
          <StatusIndicator tone="syncing" label="Loading" />
        )}
        <button className="titlebar-button" type="button" onClick={onSettings} aria-label="Settings">
          <Settings size={14} />
        </button>
        <button className="titlebar-button" type="button" onClick={onHide} aria-label="Hide palette">
          <X size={14} />
        </button>
      </span>
    </footer>
  );
}
