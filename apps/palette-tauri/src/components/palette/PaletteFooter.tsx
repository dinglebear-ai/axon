import { Bot, Clock3, Home, Settings, X } from "lucide-react";

import { Button } from "@/components/ui/aurora/button";
import { Kbd } from "@/components/ui/aurora/kbd";
import { StatusIndicator } from "@/components/ui/aurora/status-indicator";
import type { PaletteConfig } from "@/lib/axonClient";
import { hostLabel } from "@/lib/url";

interface PaletteFooterProps {
  config: PaletteConfig | null;
  configError: string | null;
  onRecent: () => void;
  onSettings: () => void;
  onCodex: () => void;
  onHide: () => void;
  onHome?: () => void;
  mobile?: boolean;
  recentActive?: boolean;
  showHide?: boolean;
}

// Footer row: keyboard hint legend on the left, endpoint status + settings/hide
// controls on the right.
export function PaletteFooter({
  config,
  configError,
  onRecent,
  onSettings,
  onCodex,
  onHide,
  onHome,
  mobile = false,
  recentActive = false,
  showHide = true,
}: PaletteFooterProps) {
  const showHints = config?.showFooterHints ?? false;

  if (mobile) {
    return (
      <nav className="palette-footer palette-footer-mobile" aria-label="Palette navigation">
        <Button
          variant="plain"
          size="unstyled"
          className="mobile-nav-item"
          type="button"
          onClick={onCodex}
          aria-label="Codex"
        >
          <Bot size={19} strokeWidth={1.9} />
          <span>Codex</span>
        </Button>
        <Button
          variant="plain"
          size="unstyled"
          className="mobile-nav-item"
          type="button"
          onClick={onHome}
          aria-label="Home"
        >
          <Home size={19} strokeWidth={1.9} />
          <span>Home</span>
        </Button>
        <Button
          variant="plain"
          size="unstyled"
          className={recentActive ? "mobile-nav-item mobile-nav-item-active" : "mobile-nav-item"}
          type="button"
          onClick={onRecent}
          aria-label="Recent"
        >
          <Clock3 size={19} strokeWidth={1.9} />
          <span>Recent</span>
        </Button>
        <Button
          variant="plain"
          size="unstyled"
          className="mobile-nav-item"
          type="button"
          onClick={onSettings}
          aria-label="Settings"
        >
          <Settings size={19} strokeWidth={1.9} />
          <span>Settings</span>
        </Button>
      </nav>
    );
  }

  return (
    <footer className="palette-footer">
      {showHints ? (
        <span className="palette-footer-hints">
          <Button
            variant="plain"
            size="unstyled"
            className="palette-recent"
            type="button"
            onClick={onRecent}
          >
            ↺ recent
          </Button>
          <span className="palette-hint-group">
            <Kbd unstyled>↑</Kbd>
            <Kbd unstyled>↓</Kbd> navigate
          </span>
          <span className="palette-hint-group">
            <Kbd unstyled>tab</Kbd> select
          </span>
          <span className="palette-hint-group">
            <Kbd unstyled>↵</Kbd> run
          </span>
          <span className="palette-hint-group">
            <Kbd unstyled>esc</Kbd> close
          </span>
        </span>
      ) : (
        <span className="palette-footer-spacer" aria-hidden="true" />
      )}
      <span className="palette-status">
        {config ? (
          <StatusIndicator
            tone="syncing"
            label={`${hostLabel(config.serverUrl)} / ${config.collection}`}
            pulse={false}
          />
        ) : configError ? (
          <StatusIndicator tone="error" label="Config error" />
        ) : (
          <StatusIndicator tone="syncing" label="Loading" />
        )}
        <Button
          variant="plain"
          size="unstyled"
          className="titlebar-button"
          type="button"
          onClick={onCodex}
          aria-label="Codex app-server"
        >
          <Bot size={14} />
        </Button>
        <Button
          variant="plain"
          size="unstyled"
          className="titlebar-button"
          type="button"
          onClick={onSettings}
          aria-label="Settings"
        >
          <Settings size={14} />
        </Button>
        {showHide && (
          <Button
            variant="plain"
            size="unstyled"
            className="titlebar-button"
            type="button"
            onClick={onHide}
            aria-label="Hide palette"
          >
            <X size={14} />
          </Button>
        )}
      </span>
    </footer>
  );
}
