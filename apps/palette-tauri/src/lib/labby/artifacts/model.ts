import type {
  ArtifactDetail,
  ArtifactFamily,
  ArtifactFile,
  ArtifactSummary,
} from "../../clients/labbyClient";

export const ARTIFACT_FAMILIES: ArtifactFamily[] = ["skill", "prompt", "agent", "hook"];
export const LIST_PAGE_SIZE = 50;
export const VIRTUALIZE_ABOVE = 100;
export const MAX_RENDERED_TEXT = 256 * 1024;
export const MAX_FILES = 64;

export interface ArtifactBuffer {
  profileId: string;
  family: ArtifactFamily;
  artifactId: string | null;
  name: string;
  visibility: "private" | "shared";
  files: ArtifactFile[];
  baseFiles: ArtifactFile[];
  base: ArtifactDetail | null;
}

const PRIMARY: Record<ArtifactFamily, string> = {
  skill: "SKILL.md",
  prompt: "PROMPT.md",
  agent: "AGENT.md",
  hook: "HOOK.json",
};

export function emptyBuffer(profileId: string, family: ArtifactFamily): ArtifactBuffer {
  const path = PRIMARY[family];
  const content =
    family === "hook"
      ? '{\n  "event": "pre_tool",\n  "command": "command-name",\n  "arguments": []\n}\n'
      : `# New ${family[0].toUpperCase()}${family.slice(1)}\n\n`;
  return {
    profileId,
    family,
    artifactId: null,
    name: `new-${family}`,
    visibility: "private",
    files: [{ path, content }],
    baseFiles: [],
    base: null,
  };
}

export function bufferFrom(
  profileId: string,
  family: ArtifactFamily,
  detail: ArtifactDetail,
  files: ArtifactFile[],
): ArtifactBuffer {
  return {
    profileId,
    family,
    artifactId: detail.artifact_id,
    name: detail.name,
    visibility: detail.visibility,
    files,
    baseFiles: files.map((file) => ({ ...file })),
    base: detail,
  };
}

export function hasUnsavedChanges(buffer: ArtifactBuffer | null): boolean {
  if (!buffer) return false;
  if (!buffer.base)
    return (
      buffer.name !== `new-${buffer.family}` ||
      buffer.files.some((file) => file.content.trim() !== "")
    );
  return (
    buffer.name !== buffer.base.name ||
    buffer.files.length !== buffer.baseFiles.length ||
    buffer.files.some((file, index) => {
      const original = buffer.baseFiles[index];
      return !original || original.path !== file.path || original.content !== file.content;
    })
  );
}

export function validateBuffer(buffer: ArtifactBuffer): string | null {
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(buffer.name))
    return "Name must be 1-128 safe identifier characters.";
  if (buffer.files.length === 0 || buffer.files.length > MAX_FILES)
    return `Artifacts require 1-${MAX_FILES} files.`;
  if (!buffer.files.some((file) => file.path === PRIMARY[buffer.family]))
    return `${buffer.family} requires ${PRIMARY[buffer.family]}.`;
  const seen = new Set<string>();
  for (const file of buffer.files) {
    if (
      !file.path ||
      file.path.startsWith("/") ||
      file.path.includes("..") ||
      file.path.includes("\\") ||
      seen.has(file.path)
    )
      return "File paths must be unique, relative, and traversal-free.";
    if (file.content.length > MAX_RENDERED_TEXT)
      return `${file.path} exceeds the 256 KiB editor limit.`;
    seen.add(file.path);
  }
  return null;
}

export function can(summary: ArtifactSummary, action: string): boolean {
  return (
    summary.can_mutate &&
    summary.allowed_actions.includes(
      `${summary.latest_revision_files.some((f) => f.path === "PROMPT.md") ? "prompt" : summary.latest_revision_files.some((f) => f.path === "AGENT.md") ? "agent" : summary.latest_revision_files.some((f) => f.path === "HOOK.json") ? "hook" : "skill"}_library.${action}`,
    )
  );
}

export function boundedText(value: string): string {
  return value.slice(0, MAX_RENDERED_TEXT);
}

export function idempotencyKey(action: string, target: string): string {
  return `palette-${action}-${target}-${crypto.randomUUID()}`.slice(0, 256);
}

export function reapply(
  local: ArtifactBuffer,
  current: ArtifactDetail,
  remoteFiles: ArtifactFile[],
): ArtifactBuffer {
  return {
    ...local,
    base: current,
    artifactId: current.artifact_id,
    baseFiles: remoteFiles.map((file) => ({ ...file })),
    files: local.files.length ? local.files : remoteFiles,
  };
}
