import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";

import { isTauriRuntime } from "@/lib/invoke";
import type { RunState } from "@/lib/runState";

type PaletteStreamEvent =
  | { type: "started"; requestId: string; path: string }
  | { type: "delta"; requestId: string; text: string }
  | { type: "done"; requestId: string; answer?: string | null }
  | { type: "error"; requestId: string; message: string };

type SetRunState = React.Dispatch<React.SetStateAction<RunState>>;

const appWindow = isTauriRuntime
  ? getCurrentWindow()
  : {
      listen: async () => () => undefined,
    };

export function usePaletteStream(setRun: SetRunState) {
  useEffect(() => {
    let disposed = false;
    const unlisten = appWindow.listen<PaletteStreamEvent>("palette://stream", (event) => {
      if (disposed) return;
      const payload = event.payload;
      if (payload.type === "delta") {
        setRun((current) =>
          current.kind === "streaming" && current.requestId === payload.requestId
            ? { ...current, text: current.text + payload.text }
            : current,
        );
      } else if (payload.type === "done") {
        setRun((current) =>
          current.kind === "streaming" && current.requestId === payload.requestId
            ? {
                kind: "success",
                title: `${current.actionLabel} completed`,
                subtitle: current.subtitle,
                text: payload.answer ?? current.text,
                outputKind: current.outputKind,
                prompt: current.prompt,
                result: {
                  ok: true,
                  status: 200,
                  path: current.path,
                  method: "POST",
                  payload: { answer: payload.answer ?? current.text },
                },
              }
            : current,
        );
      } else if (payload.type === "error") {
        setRun((current) =>
          current.kind === "streaming" && current.requestId === payload.requestId
            ? {
                kind: "error",
                title: `${current.actionLabel} failed`,
                subtitle: current.path,
                text: payload.message,
                outputKind: current.outputKind,
                prompt: current.prompt,
                result: {
                  ok: false,
                  status: 0,
                  path: current.path,
                  method: "POST",
                  payload: { error: payload.message },
                },
              }
            : current,
        );
      }
    });
    return () => {
      disposed = true;
      void unlisten.then((fn) => fn());
    };
  }, [setRun]);
}

export function newRequestId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
