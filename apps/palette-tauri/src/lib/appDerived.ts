import type { PaletteAction } from "@/lib/actions";
import type { PaletteConfig } from "@/lib/axonClient";
import { hostLabel } from "@/lib/url";

export function endpointState(config: PaletteConfig | null, configError: string | null) {
  return {
    label: config ? hostLabel(config.serverUrl) : configError ? "Config error" : "Loading",
    tone: configError ? ("error" as const) : ("syncing" as const),
  };
}

export function shouldAutoRunOnSwitch(action: PaletteAction) {
  return action.argMode === "none" && action.autoRunOnSwitch === true;
}
