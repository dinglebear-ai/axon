import { ArrowLeft, ArrowRight, Compass, Plus, RotateCw, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/aurora/button";
import { BROWSER_HOME_URL, hostLabel, isHomeUrl, normalizeBrowserUrl } from "@/lib/browserUrl";
import { invoke, isTauriRuntime } from "@/lib/invoke";

interface BrowserTab {
  id: string;
  /** The last URL navigated to. `BROWSER_HOME_URL` for a fresh/new tab. */
  url: string;
  /** Address-bar display text — may differ from `url` while the user is typing. */
  addressText: string;
}

let nextTabId = 1;
function makeTab(url: string = BROWSER_HOME_URL): BrowserTab {
  return { id: `tab-${nextTabId++}`, url, addressText: isHomeUrl(url) ? "" : url };
}

interface BrowserBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * Real in-app "Browser" tool: an address bar + tab strip that drives an
 * embedded native Tauri child `Webview` attached to the main window
 * (see `src-tauri/src/browser.rs`).
 */
export function BrowserView({
  initialTarget,
  onClose,
}: {
  initialTarget: string | null;
  onClose: () => void;
}) {
  const initialUrl = initialTarget ? normalizeBrowserUrl(initialTarget) : BROWSER_HOME_URL;
  const [tabs, setTabs] = useState<BrowserTab[]>(() => [makeTab(initialUrl)]);
  const [activeTabId, setActiveTabId] = useState(() => tabs[0].id);
  const surfaceRef = useRef<HTMLDivElement | null>(null);

  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];

  const getBounds = useCallback((): BrowserBounds | undefined => {
    if (!surfaceRef.current) return undefined;
    const rect = surfaceRef.current.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return undefined;
    return {
      x: Math.round(rect.left),
      y: Math.round(rect.top),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
    };
  }, []);

  const updateBounds = useCallback(() => {
    if (!isTauriRuntime) return;
    const bounds = getBounds();
    if (bounds) {
      void invoke("browser_set_bounds", { bounds });
    }
  }, [getBounds]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: mount/unmount-only open+close of the native child webview
  useEffect(() => {
    if (!isTauriRuntime) return;
    const bounds = getBounds();
    void invoke("browser_open", { url: activeTab.url, bounds });

    const observer = new ResizeObserver(() => {
      updateBounds();
    });
    if (surfaceRef.current) {
      observer.observe(surfaceRef.current);
    }
    window.addEventListener("resize", updateBounds);

    const rafId = requestAnimationFrame(() => {
      updateBounds();
    });
    const timerId = setTimeout(() => {
      updateBounds();
    }, 100);

    return () => {
      cancelAnimationFrame(rafId);
      clearTimeout(timerId);
      observer.disconnect();
      window.removeEventListener("resize", updateBounds);
      void invoke("browser_close");
    };
  }, []);

  function updateActiveTab(patch: Partial<BrowserTab>) {
    setTabs((current) => current.map((tab) => (tab.id === activeTabId ? { ...tab, ...patch } : tab)));
  }

  function navigate(raw: string) {
    const url = normalizeBrowserUrl(raw);
    updateActiveTab({ url, addressText: isHomeUrl(url) ? "" : url });
    if (isTauriRuntime) void invoke("browser_navigate", { url, bounds: getBounds() });
  }

  function selectTab(id: string) {
    setActiveTabId(id);
    const tab = tabs.find((candidate) => candidate.id === id);
    if (tab && isTauriRuntime) void invoke("browser_navigate", { url: tab.url, bounds: getBounds() });
  }

  function newTab() {
    const tab = makeTab();
    setTabs((current) => [...current, tab]);
    setActiveTabId(tab.id);
    if (isTauriRuntime) void invoke("browser_navigate", { url: tab.url, bounds: getBounds() });
  }

  function closeTab(id: string) {
    setTabs((current) => {
      const remaining = current.filter((tab) => tab.id !== id);
      if (remaining.length === 0) {
        const fresh = makeTab();
        setActiveTabId(fresh.id);
        if (isTauriRuntime) void invoke("browser_navigate", { url: fresh.url, bounds: getBounds() });
        return [fresh];
      }
      if (id === activeTabId) {
        const closedIndex = current.findIndex((tab) => tab.id === id);
        const next = remaining[Math.max(0, closedIndex - 1)];
        setActiveTabId(next.id);
        if (isTauriRuntime) void invoke("browser_navigate", { url: next.url, bounds: getBounds() });
      }
      return remaining;
    });
  }

  function goBack() {
    if (isTauriRuntime) void invoke("browser_back");
  }

  function goForward() {
    if (isTauriRuntime) void invoke("browser_forward");
  }

  function reload() {
    if (isTauriRuntime) void invoke("browser_reload");
  }

  function closeBrowser() {
    onClose();
  }

  return (
    <section className="browser-panel">
      <header className="browser-tabstrip">
        <Button
          variant="ghost"
          size="icon"
          type="button"
          onClick={closeBrowser}
          title="Close browser"
          aria-label="Close browser"
        >
          <X size={15} />
        </Button>
        <div className="browser-tabs aurora-scrollbar" role="tablist">
          {tabs.map((tab) => (
            <div
              key={tab.id}
              role="tab"
              aria-selected={tab.id === activeTabId}
              tabIndex={0}
              className={`browser-tab${tab.id === activeTabId ? " browser-tab-active" : ""}`}
              onClick={() => selectTab(tab.id)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  selectTab(tab.id);
                }
              }}
            >
              <span className="browser-tab-favicon" aria-hidden="true">
                {isHomeUrl(tab.url) ? <Compass size={12} /> : hostLabel(tab.url).charAt(0).toUpperCase()}
              </span>
              <span className="browser-tab-title">{isHomeUrl(tab.url) ? "New Tab" : hostLabel(tab.url)}</span>
              <Button
                variant="plain"
                size="unstyled"
                type="button"
                className="browser-tab-close"
                onClick={(event) => {
                  event.stopPropagation();
                  closeTab(tab.id);
                }}
                aria-label={`Close tab ${hostLabel(tab.url)}`}
              >
                <X size={11} />
              </Button>
            </div>
          ))}
        </div>
        <Button variant="ghost" size="icon" type="button" onClick={newTab} title="New tab" aria-label="New tab">
          <Plus size={15} />
        </Button>
      </header>

      <div className="browser-toolbar">
        <Button variant="ghost" size="icon" type="button" onClick={goBack} title="Back" aria-label="Back">
          <ArrowLeft size={15} />
        </Button>
        <Button variant="ghost" size="icon" type="button" onClick={goForward} title="Forward" aria-label="Forward">
          <ArrowRight size={15} />
        </Button>
        <Button variant="ghost" size="icon" type="button" onClick={reload} title="Reload" aria-label="Reload">
          <RotateCw size={14} />
        </Button>
        <form
          className="browser-address-form"
          onSubmit={(event) => {
            event.preventDefault();
            navigate(activeTab.addressText);
          }}
        >
          <input
            className="browser-address-input"
            type="text"
            spellCheck={false}
            placeholder="Search the web or type a URL…"
            value={activeTab.addressText}
            onChange={(event) => updateActiveTab({ addressText: event.target.value })}
            aria-label="Address bar"
          />
        </form>
      </div>

      <div className="browser-surface" ref={surfaceRef}>
        {!isTauriRuntime && (
          <div className="browser-surface-hint browser-surface-unavailable">
            <Compass size={22} />
            <strong>Browser tool requires the desktop app</strong>
            <p>Real in-app browsing runs in a native webview that only the Tauri desktop build can create.</p>
          </div>
        )}
      </div>
    </section>
  );
}
