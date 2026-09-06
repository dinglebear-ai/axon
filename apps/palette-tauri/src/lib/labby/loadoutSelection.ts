import type { BackendProfile } from "@/lib/backendProfiles/model";

export type LoadoutRunMode = "context" | "agent";

export interface LoadoutSelection {
  profileId: string;
  integrationId: string;
  loadoutId: string;
  name: string;
  expectedRevision: number;
  mode: LoadoutRunMode;
  conversationBinding: string;
}

const PREFIX = "axon.palette.loadout.v1";
const MAX_TOKEN_BYTES = 512;

function storageKey(profileId: string) {
  return `${PREFIX}:${profileId}`;
}

export function readLoadoutSelection(profile?: BackendProfile | null): LoadoutSelection | null {
  if (!profile || typeof localStorage === "undefined") return null;
  try {
    const value = JSON.parse(
      localStorage.getItem(storageKey(profile.id)) ?? "null",
    ) as LoadoutSelection | null;
    if (
      !value ||
      value.profileId !== profile.id ||
      !value.integrationId ||
      !value.loadoutId ||
      !Number.isSafeInteger(value.expectedRevision) ||
      value.expectedRevision < 1 ||
      !["context", "agent"].includes(value.mode)
    )
      return null;
    return value;
  } catch {
    return null;
  }
}

export function writeLoadoutSelection(
  profile: BackendProfile,
  value: Omit<LoadoutSelection, "profileId" | "conversationBinding">,
) {
  const selection: LoadoutSelection = {
    ...value,
    profileId: profile.id,
    conversationBinding: crypto.randomUUID(),
  };
  localStorage.setItem(storageKey(profile.id), JSON.stringify(selection));
  window.dispatchEvent(new CustomEvent("palette:loadout-selection", { detail: profile.id }));
  return selection;
}

export function clearLoadoutSelection(profile: BackendProfile) {
  localStorage.removeItem(storageKey(profile.id));
  sessionStorage.removeItem(`${storageKey(profile.id)}:delegation`);
  window.dispatchEvent(new CustomEvent("palette:loadout-selection", { detail: profile.id }));
}

export function readDelegation(profileId: string): string | null {
  const value = sessionStorage.getItem(`${storageKey(profileId)}:delegation`)?.trim() ?? "";
  return value && new TextEncoder().encode(value).length <= MAX_TOKEN_BYTES ? value : null;
}

export function writeDelegation(profileId: string, value: string) {
  const clean = value.trim();
  if (!clean || new TextEncoder().encode(clean).length > MAX_TOKEN_BYTES) {
    throw new Error("Delegation must be between 1 and 512 UTF-8 bytes.");
  }
  sessionStorage.setItem(`${storageKey(profileId)}:delegation`, clean);
}

export function loadoutRequestFields(profile?: BackendProfile | null): Record<string, unknown> {
  const selection = readLoadoutSelection(profile);
  if (!selection) return {};
  const loadout = {
    integrationId: selection.integrationId,
    loadoutId: selection.loadoutId,
    expectedRevision: selection.expectedRevision,
    conversationBinding: selection.conversationBinding,
  };
  if (selection.mode === "context") return { loadout };
  const delegationToken = readDelegation(selection.profileId);
  if (!delegationToken)
    throw new Error(
      "Agent mode requires an audience-bound Labby delegation. Add it in the loadout selector; the Axon bearer is never reused.",
    );
  return { loadout, agent: { delegationToken, maxToolCalls: 8, timeoutMs: 120_000 } };
}
