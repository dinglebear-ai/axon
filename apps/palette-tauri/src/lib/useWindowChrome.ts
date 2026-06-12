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
  if (jobMinimized) return { width: 680, height: 96 };
  if (settingsOpen) return { width: 800, height: 560 };
  if (historyOpen) return { width: 760, height: 520 };
  if (jobExpanded) {
    return {
      width: Math.min(1280, screen.width - 120),
      height: Math.min(470, screen.height - 120),
    };
  }
  if (showResultsLayout) {
    return {
      // Operation responses get a roomy, screen-relative window so long
      // answers are easy to review (still resizable + double-click maximize).
      width: Math.min(1280, screen.width - 120),
      height: Math.min(860, screen.height - 120),
    };
  }
  if (showContent) return { width: 760, height: Math.min(browseHeight(), screen.height - 80) };
  return { width: 680, height: 56 };
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
      // BROWSE_CHROME is the non-list vertical chrome: command bar, panel
      // heading, footer, and paddings. Measured exactly so the window hugs its
      // content — an over-estimate leaves dead space below the footer (it floats
      // off the bottom edge); an under-estimate clips it.
      const BROWSE_CHROME = 130;
      const viewport = document.querySelector(".action-scroll-viewport");
      if (!(viewport instanceof HTMLElement)) {
        return BROWSE_CHROME + filteredLength * 48;
      }
      // `viewport.scrollHeight` is the full (unsquashed) list height regardless of the
      // current window, so it's stable even when measured from the compact window we're
      // resizing away from. `LIST_CAP` mirrors `.action-scroll` max-height.
      const LIST_CAP = 338;
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
