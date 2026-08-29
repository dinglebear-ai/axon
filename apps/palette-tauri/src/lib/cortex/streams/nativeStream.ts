import type { BackendProfile } from "../../backendProfiles/model";
import { appWindow, invoke, isTauriRuntime } from "../../invoke";

export type CortexStreamKind = "logs" | "sessions";
export interface NativeStreamEvent {
  requestId: string;
  generation: number;
  event: string;
  id?: string | null;
  data: unknown;
}

export async function followCortexStream(
  profile: BackendProfile,
  kind: CortexStreamKind,
  params: Record<string, string>,
  generation: number,
  onEvent: (event: NativeStreamEvent) => void,
  signal: AbortSignal,
) {
  const requestId = crypto.randomUUID();
  const path = `/v1/cortex/api/streams/${kind}`;
  const unlisten = await appWindow.listen<NativeStreamEvent>(
    "palette://backend-stream",
    ({ payload }) => {
      if (payload.requestId === requestId && payload.generation === generation) onEvent(payload);
    },
  );
  const cancel = () => void invoke("backend_cancel_request", { requestId });
  signal.addEventListener("abort", cancel, { once: true });
  try {
    if (isTauriRuntime) {
      await invoke("backend_http_stream", {
        request: { profileId: profile.id, product: "cortex", requestId, generation, path, params },
      });
    } else {
      const response = await fetch(`${path}?${new URLSearchParams(params)}`, {
        headers: { accept: "text/event-stream" },
        signal,
      });
      if (!response.ok || !response.body)
        throw new Error(`stream failed with HTTP ${response.status}`);
      await parseBrowserSse(response.body, requestId, generation, onEvent);
    }
  } finally {
    signal.removeEventListener("abort", cancel);
    unlisten();
  }
}

async function parseBrowserSse(
  body: ReadableStream<Uint8Array>,
  requestId: string,
  generation: number,
  onEvent: (event: NativeStreamEvent) => void,
) {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let pending = "";
  let event = "message";
  let id: string | null = null;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    pending += decoder.decode(value, { stream: true });
    if (pending.length > 131072) throw new Error("SSE frame exceeded the 128 KiB renderer limit");
    const frames = pending.split("\n\n");
    pending = frames.pop() ?? "";
    for (const frame of frames) {
      let data = "";
      for (const line of frame.split(/\r?\n/)) {
        if (line.startsWith("event:")) event = line.slice(6).trim();
        else if (line.startsWith("id:")) id = line.slice(3).trim();
        else if (line.startsWith("data:")) data += line.slice(5).trim();
      }
      if (data) onEvent({ requestId, generation, event, id, data: JSON.parse(data) });
      event = "message";
      id = null;
    }
  }
}
