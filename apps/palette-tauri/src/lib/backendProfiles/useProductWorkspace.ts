import { useCallback, useMemo } from "react";
import type { PaletteConfig } from "../axonClient";
import { invoke } from "../invoke";
import { activeProfile, type BackendProduct } from "./model";

export function useProductWorkspace(
  config: PaletteConfig | null,
  setDraftConfig: (config: PaletteConfig) => void,
  invalidate: () => void,
  setWorkspace: (product: BackendProduct) => void,
) {
  const labbyProfile = activeProfile(
    config?.backendProfiles,
    config?.activeBackendProfiles,
    "labby",
  );
  const cortexProfile = activeProfile(
    config?.backendProfiles,
    config?.activeBackendProfiles,
    "cortex",
  );
  const availableProducts = useMemo(
    () =>
      new Set<BackendProduct>([
        "axon",
        ...(config?.backendProfiles?.some((profile) => profile.product === "labby")
          ? ["labby" as const]
          : []),
        ...(config?.backendProfiles?.some((profile) => profile.product === "cortex")
          ? ["cortex" as const]
          : []),
      ]),
    [config?.backendProfiles],
  );
  const selectWorkspace = useCallback(
    (product: BackendProduct) => {
      const url = new URL(window.location.href);
      if (product === "axon") url.searchParams.delete("workspace");
      else url.searchParams.set("workspace", product);
      window.history.replaceState(null, "", url);
      setWorkspace(product);
    },
    [setWorkspace],
  );
  const selectBackendProfile = useCallback(
    async (product: BackendProduct, profileId: string) => {
      if (!config) return;
      invalidate();
      const next = {
        ...config,
        activeBackendProfiles: { ...config.activeBackendProfiles, [product]: profileId },
      };
      setDraftConfig(await invoke<typeof next>("save_palette_settings", { settings: next }));
      window.location.reload();
    },
    [config, invalidate, setDraftConfig],
  );
  return { availableProducts, cortexProfile, labbyProfile, selectBackendProfile, selectWorkspace };
}
