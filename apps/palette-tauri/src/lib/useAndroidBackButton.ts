import { useEffect, useRef } from "react";

declare global {
  interface Window {
    __AXON_ANDROID_BACK__?: () => boolean;
  }
}

/**
 * Expose Palette's Back-navigation decision to the app-owned Android Activity.
 *
 * Tauri's Android app plugin owns the platform Back dispatcher. When no
 * `back-button` plugin listener is registered it delegates to
 * `Activity.onBackPressed()`, which our generated MainActivity overrides and
 * forwards into this synchronous WebView callback.
 *
 * Return `true` when Palette consumed Back by unwinding a nested view. Return
 * `false` at the root so MainActivity can finish the Activity natively.
 */
export function useAndroidBackButton(enabled: boolean, onBack: () => boolean) {
  const onBackRef = useRef(onBack);
  onBackRef.current = onBack;

  useEffect(() => {
    if (!enabled) return;

    const handler = () => onBackRef.current();
    window.__AXON_ANDROID_BACK__ = handler;

    return () => {
      if (window.__AXON_ANDROID_BACK__ === handler) {
        delete window.__AXON_ANDROID_BACK__;
      }
    };
  }, [enabled]);
}
