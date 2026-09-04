import { platform } from "@tauri-apps/plugin-os";
import { ACTIONS, MOBILE_ACTIONS } from "./actions";
import type { BackendProduct } from "./backendProfiles/model";
import { isTauriRuntime } from "./invoke";

export const shortcutOptions = [
  "Ctrl+Shift+Space",
  "Alt+Space",
  "Ctrl+Space",
  "Cmd+Shift+Space",
] as const;
const runtimePlatform = isTauriRuntime ? platform() : null;
export const androidRuntime = runtimePlatform === "android";
const mobilePreview =
  !isTauriRuntime &&
  (import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV === true &&
  new URLSearchParams(window.location.search).get("mobile-preview") === "1";
export const mobileRuntime = androidRuntime || runtimePlatform === "ios" || mobilePreview;
export const runtimeActions = mobileRuntime ? MOBILE_ACTIONS : ACTIONS;
export const initialWorkspace = (): BackendProduct => {
  const value = new URLSearchParams(window.location.search).get("workspace");
  return value === "labby" || value === "cortex" ? value : "axon";
};
document.documentElement.classList.toggle("tauri-runtime", isTauriRuntime || mobilePreview);
document.documentElement.classList.toggle("tauri-mobile-runtime", mobileRuntime);
