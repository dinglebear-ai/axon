import type { BackendProfile } from "../backendProfiles/model";
import { backendRequest } from "./backendTransport";
export class CortexClient {
  constructor(readonly profile: BackendProfile) {
    if (profile.product !== "cortex") throw new Error("CortexClient requires a Cortex profile");
  }
  request<T>(
    method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE",
    path: `/v1/cortex/${string}`,
    body?: unknown,
    signal?: AbortSignal,
  ) {
    return backendRequest<T>(this.profile, method, path, body, signal);
  }
}
