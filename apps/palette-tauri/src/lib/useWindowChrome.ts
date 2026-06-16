import { useEffect, useRef } from "react";

import { invoke } from "@/lib/invoke";

interface WindowChromeArgs {
  jobExpanded: boolean;
  jobMinimized: boolean;
  settingsOpen: boolean;
  historyOpen: boolean;
  showResultsLayout: boolean;
  showContent: boolean;
  filteredLength: number;
  shownTick: number;
}

interface PaletteScreen {
  width: number;
  height: number;
}

type BrowseHeight = () => number;

// ── Named window-chrome constants ────────────────────────────────────────────
// The palette is a borderless window resized to hug each view; these are the
// per-view logical-px dimensions. `resize_palette` sizes in logical px, so CSS-px
// measurements map 1:1 across DPIs.
const COMPACT = { width: 680, height: 56 }; // launcher input only
const TRAY = { width: 680, height: 96 }; // minimized crawl-job tray
const SETTINGS = { width: 800, height: 560 };
const HISTORY = { width: 760, height: 520 };
const BROWSE_WIDTH = 760; // action-list browse view
const JOB_MAX = { width: 1280, height: 470 }; // expanded crawl-job card (screen-relative cap)
const RESULTS_MAX = { width: 1280, height: 860 }; // operation-result view (screen-relative cap)
const SCREEN_MARGIN = 120; // gap kept from screen edges for the roomy views
const BROWSE_SCREEN_MARGIN = 80; // gap kept from screen edges for the browse view

// Selector for the scrollable action-list viewport whose rendered height we
// measure to size the browse window. A ref would be cleaner (finding L3) but the
// element is rendered by ActionList and threading a ref through would change this
// hook's public args (consumed by App.tsx); named here instead.
const ACTION_SCROLL_VIEWPORT_SELECTOR = ".action-scroll-viewport";

// Per-item fallback row height when the rendered viewport can't be measured.
const FALLBACK_ROW_HEIGHT = 48;

// BROWSE_CHROME is the non-list vertical chrome: command bar, panel heading,
// footer, and paddings. Measured exactly so the window hugs its content — an
// over-estimate leaves dead space below the footer (it floats off the bottom
// edge); an under-estimate clips it.
const BROWSE_CHROME = 130;

// LIST_CAP mirrors .action-scroll max-height in styles.css — keep in sync.
// `viewport.scrollHeight` is the full (unsquashed) list height regardless of the
// current window, so it's stable even when measured from the compact window we're
// resizing away from.
const LIST_CAP = 338;

export function resolvePaletteWindowSize(
  {
    jobExpanded,
    jobMinimized,
    settingsOpen,
    historyOpen,
    showResultsLayout,
    showContent,
  }: Omit<WindowChromeArgs, "shownTick">,
  screen: PaletteScreen,
  browseHeight: BrowseHeight,
): { width: number; height: number } {
  if (jobMinimized) return TRAY;
  if (settingsOpen) return SETTINGS;
  if (historyOpen) return HISTORY;
  if (jobExpanded) {
    return {
      width: Math.min(JOB_MAX.width, screen.width - SCREEN_MARGIN),
      height: Math.min(JOB_MAX.height, screen.height - SCREEN_MARGIN),
    };
  }
  if (showResultsLayout) {
    return {
      // Operation responses get a roomy, screen-relative window so long
      // answers are easy to review (still resizable + double-click maximize).
      width: Math.min(RESULTS_MAX.width, screen.width - SCREEN_MARGIN),
      height: Math.min(RESULTS_MAX.height, screen.height - SCREEN_MARGIN),
    };
  }
  if (showContent) {
    return { width: BROWSE_WIDTH, height: Math.min(browseHeight(), screen.height - BROWSE_SCREEN_MARGIN) };
  }
  return COMPACT;
}

// Owns the native window's size/visibility behavior for the palette: it resizes
// the borderless window to fit the current view, and suppresses hide-on-blur
// while a result/settings/history view is open so the window doesn't vanish when
// the user drags to resize it or clicks another window to review a response.
export function useWindowChrome({
  jobExpanded,
  jobMinimized,
  settingsOpen,
  historyOpen,
  showResultsLayout,
  showContent,
  filteredLength,
  shownTick,
}: WindowChromeArgs) {
  const lastSizeRef = useRef<{ width: number; height: number } | null>(null);
  const lastShownTickRef = useRef(shownTick);

  useEffect(() => {
    // The browse card hugs its content: the action list is capped (max-height
    // 338px, see `.action-scroll`) and scrolls, so a per-item formula overshoots
    // the real height and leaves a transparent gap below the footer in the
    // borderless window. Measure the rendered content instead. `resize_palette`
    // sizes the window in logical px, so CSS-px measurements map 1:1 across DPIs.
    const browseHeight = () => {
      const viewport = document.querySelector(ACTION_SCROLL_VIEWPORT_SELECTOR);
      if (!(viewport instanceof HTMLElement)) {
        return BROWSE_CHROME + filteredLength * FALLBACK_ROW_HEIGHT;
      }
      return BROWSE_CHROME + Math.min(viewport.scrollHeight, LIST_CAP);
    };
    const size = resolvePaletteWindowSize(
      {
        jobExpanded,
        jobMinimized,
        settingsOpen,
        historyOpen,
        showResultsLayout,
        showContent,
        filteredLength,
      },
      { width: window.screen.availWidth, height: window.screen.availHeight },
      browseHeight,
    );
    // `show_palette` hard-sets the window to 680×56 on every show and bumps
    // `shownTick`. If React state is still a results view, the dedup below would
    // otherwise see the unchanged stale size and skip — leaving the window stuck
    // at 56px while the results layout renders (a clipped strip). Force a resize
    // whenever the window was just shown so the real size is re-applied.
    const justShown = lastShownTickRef.current !== shownTick;
    lastShownTickRef.current = shownTick;
    // Skip redundant resizes: while typing, the browse height is constant, so this
    // effect re-runs on every keystroke (filteredLength) with the same size — and
    // each resize_palette also re-centers the window, which would jitter it.
    if (
      !justShown &&
      lastSizeRef.current?.width === size.width &&
      lastSizeRef.current?.height === size.height
    ) {
      return;
    }
    lastSizeRef.current = size;
    void invoke("resize_palette", size);
  }, [jobExpanded, jobMinimized, settingsOpen, historyOpen, showResultsLayout, showContent, filteredLength, shownTick]);

  // Launcher states (compact/browse/mode) keep click-away-to-dismiss; while a
  // result/settings/history view is open we keep the window up so resizing it
  // (which can steal focus) or copying from another window won't make it vanish.
  useEffect(() => {
    void invoke("set_blur_dismiss", { enabled: !showResultsLayout });
  }, [showResultsLayout]);
}
