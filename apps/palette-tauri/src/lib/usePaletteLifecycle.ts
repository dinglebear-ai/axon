import { useEffect, type Dispatch, type SetStateAction } from "react";

import { appWindow, invoke } from "@/lib/invoke";
import { focusInput } from "@/lib/paletteView";
import type { ViewIntent } from "@/lib/paletteViewState";

/** Wires Tauri palette window events and blur dismissal. */
export function usePaletteLifecycle(
  dispatchView: Dispatch<ViewIntent>,
  setShownTick: Dispatch<SetStateAction<number>>,
) {
  useEffect(() => {
    const unlisteners = [
      appWindow.listen("palette://shown", () => {
        setShownTick((tick) => tick + 1);
        focusInput(true);
      }),
      appWindow.listen("palette://open-settings", () => dispatchView({ type: "openSettings" })),
    ];
    return () => {
      void Promise.all(unlisteners).then((items) => items.forEach((unlisten) => unlisten()));
    };
  }, [dispatchView, setShownTick]);

  useEffect(() => {
    const onBlur = () => void invoke("hide_palette");
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, []);
}
