// Pure types/helpers for the SFTP connection-profile UI. Mirrors filesModel.ts's
// shape (types + pure helpers, no component logic) per the palette convention.

export interface SftpConnectionProfile {
  id: string;
  label: string;
  host: string;
  port: number;
  username: string;
  privateKeyPath: string;
}

export type SftpConnectionDraft = Omit<SftpConnectionProfile, "id">;

export function createEmptyConnectionDraft(): SftpConnectionDraft {
  return { label: "", host: "", port: 22, username: "", privateKeyPath: "" };
}

export function isValidConnectionDraft(draft: SftpConnectionDraft): boolean {
  return (
    draft.host.trim().length > 0 &&
    draft.username.trim().length > 0 &&
    draft.privateKeyPath.trim().length > 0 &&
    draft.port >= 1 &&
    draft.port <= 65535
  );
}

export interface SftpEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  modifiedUnix?: number | null;
}

export interface SftpKnownHostEntry {
  host: string;
  port: number;
  keyType: string;
  fingerprint: string;
  firstSeenUnix: number;
}
