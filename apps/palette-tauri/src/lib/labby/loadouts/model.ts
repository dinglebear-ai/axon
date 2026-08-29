import type {
  CapabilityFamily,
  CapabilityRef,
  ExecutionLoadoutDraft,
  LabbyCatalogEntry,
} from "../../clients/labbyClient";

export const CAPABILITY_FAMILIES: CapabilityFamily[] = [
  "tool",
  "prompt",
  "resource",
  "skill",
  "agent",
  "mcp_app",
  "mcp_server",
  "plugin",
];
export const MAX_LOADOUT_MEMBERS = 512;
export const MAX_LOADOUT_TEXT_BYTES = 256;
export const VIRTUALIZE_CATALOG_ABOVE = 100;

export interface LoadoutBuffer {
  profileId: string;
  base: ExecutionLoadoutDraft;
  name: string;
  description: string;
  members: CapabilityRef[];
}

export function bufferFrom(profileId: string, draft: ExecutionLoadoutDraft): LoadoutBuffer {
  return {
    profileId,
    base: draft,
    name: draft.name,
    description: draft.description ?? "",
    members: [...draft.members],
  };
}

export function validateBuffer(buffer: LoadoutBuffer): string | null {
  const encoder = new TextEncoder();
  if (!buffer.name.trim()) return "Name is required.";
  if (
    encoder.encode(buffer.name).length > MAX_LOADOUT_TEXT_BYTES ||
    encoder.encode(buffer.description).length > MAX_LOADOUT_TEXT_BYTES
  )
    return "Name and description are limited to 256 UTF-8 bytes.";
  if (buffer.members.length > MAX_LOADOUT_MEMBERS) return "A loadout is limited to 512 members.";
  const keys = new Set<string>();
  for (const member of buffer.members) {
    if (!member.provider || !member.memberId || !member.expectedRevision)
      return "Every member requires provider, opaque ID, and expected revision.";
    const key = capabilityKey(member);
    if (keys.has(key)) return `Duplicate member: ${member.memberId}`;
    keys.add(key);
  }
  return null;
}

export function capabilityKey(value: CapabilityRef): string {
  return `${value.provider}\u0000${value.family}\u0000${value.memberId}\u0000${value.expectedRevision}`;
}
export function catalogCapability(entry: LabbyCatalogEntry): CapabilityRef | null {
  if (entry.kind !== "mcpTool") return null;
  return {
    provider: entry.source,
    family: "tool",
    memberId: entry.id,
    expectedRevision: entry.contractHash,
  };
}
export function changedFields(buffer: LoadoutBuffer, current: ExecutionLoadoutDraft): string[] {
  const fields: string[] = [];
  if (buffer.name !== current.name) fields.push("name");
  if (buffer.description !== (current.description ?? "")) fields.push("description");
  if (JSON.stringify(buffer.members) !== JSON.stringify(current.members)) fields.push("members");
  return fields;
}
export function reapplyBuffer(
  buffer: LoadoutBuffer,
  current: ExecutionLoadoutDraft,
): LoadoutBuffer {
  return { ...buffer, base: current };
}
