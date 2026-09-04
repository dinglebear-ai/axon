import type { BackendProduct, BackendProfile } from "../backendProfiles/model";
import { invoke } from "../invoke";

export interface BackendResult<T> {
  ok: boolean;
  status: number;
  profileId: string;
  product: BackendProduct;
  requestId: string;
  payload: T;
}
export type BackendMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

export async function backendRequest<T>(
  profile: BackendProfile,
  method: BackendMethod,
  path: string,
  body: unknown = null,
  signal?: AbortSignal,
): Promise<BackendResult<T>> {
  const requestId = crypto.randomUUID();
  const cancel = () => {
    void invoke("backend_cancel_request", { requestId });
  };
  signal?.addEventListener("abort", cancel, { once: true });
  try {
    if (signal?.aborted) throw new DOMException("Request cancelled", "AbortError");
    return await invoke<BackendResult<T>>("backend_http_request", {
      request: { profileId: profile.id, product: profile.product, requestId, method, path, body },
    });
  } finally {
    signal?.removeEventListener("abort", cancel);
  }
}
