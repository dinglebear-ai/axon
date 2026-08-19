import { onBackButtonPress } from "@tauri-apps/api/app";
import { useEffect, useRef } from "react";

/**
 * Route Android's native Back button into Palette navigation.
 *
 * Registering a Tauri back-button listener suppresses Android's default
 * Activity finish, so the caller is responsible for closing the root view.
 */
export function useAndroidBackButton(enabled: boolean, onBack: () => void) {
  const onBackRef = useRef(onBack);
  onBackRef.current = onBack;

  useEffect(() => {
    if (!enabled) return;

    let disposed = false;
    let listener: Awaited<ReturnType<typeof onBackButtonPress>> | null = null;

    void onBackButtonPress(() => onBackRef.current())
      .then((registered) => {
        if (disposed) {
          void registered.unregister();
        } else {
          listener = registered;
        }
      })
      .catch((error) => {
        console.warn("failed to register Android back-button handler", error);
      });

    return () => {
      disposed = true;
      if (listener) void listener.unregister();
    };
  }, [enabled]);
}
