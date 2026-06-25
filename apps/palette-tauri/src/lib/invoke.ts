// Single invoke wrapper used by every caller (App, axonClient).
//
// In the Tauri runtime it forwards to the real `@tauri-apps/api/core` invoke.
// In a plain browser (vite dev — used for design iteration/screenshots) it falls
// back to real same-origin HTTP for `axon_http_request`, which the vite proxy
// forwards to a live `axon serve`. This keeps a single code path so things like
// `executeAction` work identically in dev and production.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export const isTauriRuntime =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// Shared Tauri window handle with a browser fallback. In the Tauri runtime it is
// the real window (event listeners wired); under `vite dev` it is a no-op stub so
// `appWindow.listen(...)` is always callable. Consumed by App's window-event
// effect and the ask-stream effect in useActionRunner.
export const appWindow = isTauriRuntime
  ? getCurrentWindow()
  : {
      listen: async () => () => undefined,
    };

export async function invoke<T = unknown>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (isTauriRuntime) return tauriInvoke<T>(command, args);
  switch (command) {
    case "axon_http_request": {
      const req = (args?.request ?? {}) as { method?: string; path?: string; body?: unknown };
      const method = (req.method ?? "GET").toUpperCase();
      const init: RequestInit = { method, headers: { accept: "application/json" } };
      if (req.body != null && method !== "GET" && method !== "DELETE") {
        init.headers = { ...(init.headers as Record<string, string>), "content-type": "application/json" };
        init.body = JSON.stringify(req.body);
      }
      const resp = await fetch(req.path ?? "/", init);
      const text = await resp.text();
      let payload: unknown = null;
      try {
        payload = text ? JSON.parse(text) : null;
      } catch {
        payload = text;
      }
      return { ok: resp.ok, status: resp.status, path: req.path ?? "", method, payload } as T;
    }
    case "load_palette_config":
    case "load_palette_default_config":
      return {
        serverUrl: "http://127.0.0.1:8001",
        token: null,
        shortcut: "Ctrl+Shift+Space",
        collection: "axon",
        resultLimit: 10,
        theme: "dark",
        hideOnBlur: false,
        openResultsInline: true,
        envValues: {},
        configValues: {},
      } as T;
    case "save_palette_settings":
      return (args?.settings ?? args) as T;
    case "hide_palette":
    case "show_palette":
    case "resize_palette":
    case "set_blur_dismiss":
      return undefined as T;
    case "axon_oauth_status":
    case "axon_oauth_logout":
      return { signedIn: false, scope: null, expiresAtUnix: null, serverUrl: null } as T;
    case "axon_oauth_login":
      throw new Error("OAuth login is only available in the desktop app");
    default:
      throw new Error(`${command} is only available in the Tauri runtime`);
  }
}
