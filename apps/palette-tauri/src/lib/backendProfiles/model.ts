export type BackendProduct = "axon" | "labby" | "cortex";

export interface BackendProfile {
  id: string;
  label: string;
  product: BackendProduct;
  origin: string;
  credentialHandle: string | null;
  credentialGeneration?: string | null;
  pinnedServerId: string | null;
  acceptedApiMajor: 1;
}

export interface ProductIdentity {
  contract_version: "1.0.0";
  product: BackendProduct;
  server_id: string;
  product_version: string;
  api_version: { major: number; minor: number };
  capabilities: string[];
  auth: { modes: string[]; credential_generation: string };
  streams: { transport: "none" | "sse"; resume: "none" | "opaque_cursor" };
}

export function assertCompatibleIdentity(
  profile: BackendProfile,
  identity: ProductIdentity,
): ProductIdentity {
  if (identity.contract_version !== "1.0.0" || identity.product !== profile.product) {
    throw new Error("Backend identity does not match the selected product profile");
  }
  if (identity.api_version.major !== profile.acceptedApiMajor) {
    throw new Error(`Unsupported ${profile.product} API major ${identity.api_version.major}`);
  }
  if (profile.pinnedServerId && identity.server_id !== profile.pinnedServerId) {
    throw new Error("Backend server identity changed; explicit re-trust is required");
  }
  return identity;
}

export function activeProfile(
  profiles: BackendProfile[] | undefined,
  active: Partial<Record<BackendProduct, string>> | undefined,
  product: BackendProduct,
): BackendProfile | null {
  const id = active?.[product];
  if (!id) return null;
  return profiles?.find((profile) => profile.product === product && profile.id === id) ?? null;
}
