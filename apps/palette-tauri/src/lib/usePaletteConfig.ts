import { useEffect, useState, type Dispatch } from "react";

import type { PaletteConfig } from "@/lib/axonClient";
import { focusInput } from "@/lib/paletteView";
import type { ViewIntent } from "@/lib/paletteViewState";
import { invoke } from "@/lib/invoke";

/** Loads palette settings, applies theme changes, and persists edits. */
export function usePaletteConfig(dispatchView: Dispatch<ViewIntent>) {
  const [config, setConfig] = useState<PaletteConfig | null>(null);
  const [draftConfig, setDraftConfig] = useState<PaletteConfig | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);

  useEffect(() => {
    invoke<PaletteConfig>("load_palette_config")
      .then((nextConfig) => {
        setConfig(nextConfig);
        setDraftConfig(nextConfig);
      })
      .catch((err) => {
        setConfigError(String(err));
        void invoke<PaletteConfig>("load_palette_default_config")
          .then((fallbackConfig) => {
            setConfig(fallbackConfig);
            setDraftConfig(fallbackConfig);
          })
          .catch(() => {
            setConfig(null);
            setDraftConfig(null);
          });
      });
  }, []);

  useEffect(() => {
    if (!config) return;
    const root = document.documentElement;
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const applyTheme = () => {
      const useLight = config.theme === "light" || (config.theme === "system" && media.matches);
      root.classList.toggle("light", useLight);
      root.classList.toggle("dark", !useLight);
    };
    applyTheme();
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [config]);

  async function saveSettings() {
    if (!draftConfig) return;
    try {
      const nextConfig = await invoke<PaletteConfig>("save_palette_settings", { settings: draftConfig });
      setConfig(nextConfig);
      setDraftConfig(nextConfig);
      setConfigError(null);
      dispatchView({ type: "closeSettings" });
      focusInput(true);
    } catch (err) {
      setConfigError(String(err));
    }
  }

  return { config, draftConfig, setDraftConfig, configError, saveSettings };
}
