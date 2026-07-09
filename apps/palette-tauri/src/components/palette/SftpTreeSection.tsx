import { forwardRef, useCallback, useImperativeHandle, useState } from "react";

import { formatBytes, type FileEntry } from "@/lib/filesModel";
import { invoke } from "@/lib/invoke";
import type { SftpConnectionProfile, SftpEntry } from "@/lib/sftpModel";
import { EntryIcon } from "./EntryIcon";

/** Mirrors `MAX_SFTP_DIR_ENTRIES` in sftp_bridge/commands.rs — used only for
 * the truncation hint text, not enforcement (the backend is authoritative). */
const MAX_SFTP_DIR_ENTRIES_HINT = 2000;

export interface SftpTreeSectionHandle {
  loadDir: (connectionId: string, path: string) => void;
  reset: () => void;
}

/**
 * The SFTP tree-browsing UI shown alongside the local file tree when a
 * connection is active. SFTP is v1 read-only browsing: a single active
 * connection at a time (see Task 5d's Open Question resolution — connecting a
 * new profile disconnects the previous one). Cwd/entries/selected/truncated
 * state is local to this component (not part of `filesViewState.ts`'s
 * reducer) since it's ephemeral remote-browsing UI state — extracting to its
 * own component file with local state is the smaller, safer change than
 * threading it through the shared reducer.
 *
 * The parent (FilesView) still owns connect/disconnect lifecycle and the
 * pane/loadGen bookkeeping needed to safely apply (or drop, if superseded) a
 * remote file read, so it drives directory loads and state resets here via an
 * imperative handle rather than lifting this state up wholesale.
 *
 * Note: `activeConnectionId` (the live backend session id) is a distinct key
 * from `activeProfile.id` (a synthesized `host:port:username` identity used
 * only to dedupe/persist saved profiles) — directory loads/opens must use
 * `activeConnectionId`, and the section's visibility gates on it too (a
 * profile lookup can legitimately miss, e.g. before `sftp/connected` has
 * updated `connections`).
 */
export const SftpTreeSection = forwardRef<
  SftpTreeSectionHandle,
  {
    activeConnectionId: string | null;
    activeProfile: SftpConnectionProfile | undefined;
    /** Called when the user opens a remote file (not a directory). */
    onOpenFile: (connectionId: string, entry: SftpEntry) => void;
  }
>(function SftpTreeSection({ activeConnectionId, activeProfile, onOpenFile }, ref) {
  const [sftpCwd, setSftpCwd] = useState("");
  const [sftpEntries, setSftpEntries] = useState<SftpEntry[]>([]);
  const [sftpSelected, setSftpSelected] = useState<SftpEntry | null>(null);
  // Set when the backend truncated a directory listing at MAX_SFTP_DIR_ENTRIES
  // (see sftp_bridge/commands.rs) — surfaced so a truncated remote listing
  // doesn't silently look like a complete one.
  const [sftpTruncated, setSftpTruncated] = useState(false);

  const loadSftpDir = useCallback((connectionId: string, path: string) => {
    invoke<{ path: string; entries: SftpEntry[]; truncated?: boolean }>("sftp_list_dir", {
      connectionId,
      path: path || null,
    })
      .then((listing) => {
        setSftpCwd(listing.path);
        setSftpEntries(listing.entries);
        setSftpTruncated(Boolean(listing.truncated));
      })
      .catch(() => {
        setSftpEntries([]);
        setSftpTruncated(false);
      });
  }, []);

  const reset = useCallback(() => {
    setSftpCwd("");
    setSftpEntries([]);
    setSftpSelected(null);
    setSftpTruncated(false);
  }, []);

  useImperativeHandle(ref, () => ({ loadDir: loadSftpDir, reset }), [loadSftpDir, reset]);

  function handleOpenEntry(entry: SftpEntry) {
    if (!activeConnectionId) return;
    if (entry.isDir) {
      loadSftpDir(activeConnectionId, entry.path);
      setSftpSelected(null);
      return;
    }
    setSftpSelected(entry);
    onOpenFile(activeConnectionId, entry);
  }

  if (!activeConnectionId) return null;

  return (
    <div className="files-sftp-section">
      <div className="files-sftp-section-header">
        <span className="files-sftp-connected-dot" aria-hidden="true" title="Connected" />
        SFTP · {activeProfile?.label}
        {sftpCwd && <span className="files-sftp-cwd"> · /{sftpCwd}</span>}
      </div>
      {sftpEntries.length === 0 ? (
        <div className="files-empty operation-muted">Empty directory</div>
      ) : (
        sftpEntries.map((entry) => (
          <button
            key={entry.path}
            type="button"
            role="option"
            aria-selected={sftpSelected?.path === entry.path}
            className={`files-row files-row-sftp${sftpSelected?.path === entry.path ? " files-row-active" : ""}`}
            onClick={() => handleOpenEntry(entry)}
          >
            <EntryIcon entry={{ ...entry, origin: "sftp" } as FileEntry} />
            <span className="files-row-name">{entry.name}</span>
            {!entry.isDir && <span className="files-row-size">{formatBytes(entry.size)}</span>}
          </button>
        ))
      )}
      {sftpTruncated && (
        <div className="files-empty operation-muted">
          Listing truncated at {MAX_SFTP_DIR_ENTRIES_HINT} entries
        </div>
      )}
    </div>
  );
});
