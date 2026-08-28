import type { BackendProfile } from "../backendProfiles/model";
import { backendRequest } from "./backendTransport";
export class LabbyClient {
  constructor(readonly profile: BackendProfile) {
    if (profile.product !== "labby") throw new Error("LabbyClient requires a Labby profile");
  }
  request<T>(
    method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE",
    path: `/v1/labby/${string}`,
    body?: unknown,
    signal?: AbortSignal,
  ) {
    return backendRequest<T>(this.profile, method, path, body, signal);
  }
}
