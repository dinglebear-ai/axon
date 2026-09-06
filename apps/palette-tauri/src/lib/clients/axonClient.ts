import type { BackendProfile } from "../backendProfiles/model";
import { backendRequest } from "./backendTransport";
export class AxonClient {
  constructor(readonly profile: BackendProfile) {
    if (profile.product !== "axon") throw new Error("AxonClient requires an Axon profile");
  }
  request<T>(
    method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE",
    path: `/v1/${string}`,
    body?: unknown,
    signal?: AbortSignal,
  ) {
    return backendRequest<T>(this.profile, method, path, body, signal);
  }
}
