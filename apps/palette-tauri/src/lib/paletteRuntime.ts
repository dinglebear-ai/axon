export type PaletteStreamEvent =
  | { type: "started"; requestId: string; path: string }
  | { type: "delta"; requestId: string; text: string }
  | { type: "done"; requestId: string; answer?: string | null }
  | { type: "error"; requestId: string; message: string };

export const shortcutOptions = [
  "Ctrl+Shift+Space",
  "Alt+Space",
  "Ctrl+Space",
  "Cmd+Shift+Space",
] as const;

export function newRequestId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
