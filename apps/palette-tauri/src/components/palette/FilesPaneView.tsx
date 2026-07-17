import {
  ChevronRight,
  Columns2,
  Loader2,
  Plug,
  PlugZap,
  RefreshCw,
  Save,
  Sparkles,
  Upload,
} from "lucide-react";
import type { Ref } from "react";

import { Button } from "@/components/ui/aurora/button";
import {
  breadcrumbSegments,
  type DirListing,
  type FileContents,
  type FileEntry,
  type FilesPane,
  formatBytes,
  formatModified,
  isChecked,
  isIndexable,
  isMarkdownLike,
  type LoadState,
} from "@/lib/filesModel";
import type { SftpConnectionProfile, SftpEntry } from "@/lib/sftpModel";
import { AiEditPanel } from "./AiEditPanel";
import { EntryIcon } from "./EntryIcon";
import { SftpTreeSection, type SftpTreeSectionHandle } from "./SftpTreeSection";

type IndexState =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "done"; ok: boolean; message: string };

/**
 * One pane of the FilesView split view: toolbar (breadcrumb, split/SFTP/
 * refresh controls), the directory tree (plus the SFTP tree section on the
 * left pane), and the file preview/edit/AI-edit area. Pure presentational —
 * all state is owned by the parent `FilesView` and reaches this component as
 * props; every interaction is reported back via callback props.
 */
export function FilesPaneView({
  pane,
  listing,
  entries,
  indexState,
  isLeftPane,
  splitOpen,
  treeWidth,
  checked,
  client,
  config,
  activeSftpConnectionId,
  activeSftpProfile,
  sftpTreeRef,
  onOpenEntry,
  onOpenSftpFile,
  onToggleChecked,
  onSetCwd,
  onGoToBreadcrumb,
  onActivatePane,
  onToggleSplit,
  onToggleSftp,
  onRefresh,
  onSetEditing,
  onCancelEdit,
  onDraftChange,
  onSave,
  onIndex,
  onSparkleToggle,
  onSparkleQueryChange,
  onSparkleSubmit,
  onProposalDeny,
  onProposalApprove,
}: {
  pane: FilesPane;
  listing: LoadState<DirListing>;
  entries: FileEntry[];
  indexState: IndexState;
  isLeftPane: boolean;
  splitOpen: boolean;
  treeWidth: number;
  checked: ReadonlySet<string>;
  client: unknown;
  config: unknown;
  activeSftpConnectionId: string | null;
  activeSftpProfile: SftpConnectionProfile | undefined;
  sftpTreeRef: Ref<SftpTreeSectionHandle>;
  onOpenEntry: (entry: FileEntry) => void;
  onOpenSftpFile: (connectionId: string, entry: SftpEntry) => void;
  onToggleChecked: (path: string) => void;
  onSetCwd: (cwd: string) => void;
  onGoToBreadcrumb: (index: number) => void;
  onActivatePane: () => void;
  onToggleSplit: () => void;
  onToggleSftp: () => void;
  onRefresh: () => void;
  onSetEditing: (editing: boolean) => void;
  onCancelEdit: () => void;
  onDraftChange: (value: string) => void;
  onSave: () => void;
  onIndex: () => void;
  onSparkleToggle: () => void;
  onSparkleQueryChange: (value: string) => void;
  onSparkleSubmit: () => void;
  onProposalDeny: () => void;
  onProposalApprove: () => void;
}) {
  const segments = breadcrumbSegments(pane.cwd);

  // Pane activation on mousedown is a pointer-only convenience for the split
  // view (mirrors clicking anywhere in a window to focus it) — the pane's
  // own focusable controls (rows, buttons, textarea) remain independently
  // keyboard-reachable via normal tab order, so no keyboard equivalent is
  // lost here.
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: see comment above.
    <div
      className="files-pane"
      style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}
      onMouseDown={onActivatePane}
    >
      <div className="files-toolbar">
        <nav className="files-breadcrumb" aria-label="Current directory">
          <Button variant="plain" size="unstyled" type="button" onClick={() => onSetCwd("")}>
            ~
          </Button>
          {segments.map((segment, index) => (
            <span key={segments.slice(0, index + 1).join("/")} className="files-breadcrumb-segment">
              <ChevronRight size={12} />
              <Button
                variant="plain"
                size="unstyled"
                type="button"
                onClick={() => onGoToBreadcrumb(index)}
              >
                {segment}
              </Button>
            </span>
          ))}
        </nav>
        {isLeftPane && (
          <Button
            variant="plain"
            size="unstyled"
            type="button"
            title={splitOpen ? "Close split" : "Split view"}
            aria-label={splitOpen ? "Close split" : "Split view"}
            onClick={onToggleSplit}
          >
            <Columns2 size={14} />
          </Button>
        )}
        {isLeftPane && (
          <Button
            variant="plain"
            size="unstyled"
            type="button"
            title={activeSftpConnectionId ? "Disconnect SFTP" : "Connect SFTP"}
            aria-label={activeSftpConnectionId ? "Disconnect SFTP" : "Connect SFTP"}
            onClick={onToggleSftp}
          >
            {activeSftpConnectionId ? <PlugZap size={14} /> : <Plug size={14} />}
          </Button>
        )}
        <Button
          variant="plain"
          size="unstyled"
          type="button"
          onClick={onRefresh}
          title="Refresh"
          aria-label="Refresh directory listing"
        >
          <RefreshCw size={14} />
        </Button>
      </div>
      <div className="files-body">
        <div
          className="files-tree aurora-scrollbar"
          role="listbox"
          aria-label="Directory entries"
          style={{ width: treeWidth, flex: `0 0 ${treeWidth}px` }}
        >
          {listing.kind === "loading" ? (
            <div className="files-empty">
              <Loader2 size={16} className="files-spin" />
              <span>Loading...</span>
            </div>
          ) : listing.kind === "error" ? (
            <div className="files-empty operation-muted">{listing.message}</div>
          ) : entries.length === 0 ? (
            <div className="files-empty operation-muted">Empty directory</div>
          ) : (
            entries.map((entry) => (
              <button
                key={entry.path}
                type="button"
                role="option"
                aria-selected={pane.selected?.path === entry.path}
                className={`files-row${pane.selected?.path === entry.path ? " files-row-active" : ""}`}
                onClick={() => onOpenEntry(entry)}
              >
                {!entry.isDir && (
                  <input
                    type="checkbox"
                    className="files-row-checkbox"
                    aria-label="Select for bulk indexing"
                    checked={isChecked(checked, entry.path)}
                    onClick={(event) => event.stopPropagation()}
                    onChange={() => onToggleChecked(entry.path)}
                  />
                )}
                <EntryIcon entry={entry} />
                <span className="files-row-name">{entry.name}</span>
                {!entry.isDir && <span className="files-row-size">{formatBytes(entry.size)}</span>}
              </button>
            ))
          )}
          {isLeftPane && (
            <SftpTreeSection
              ref={sftpTreeRef}
              activeConnectionId={activeSftpConnectionId}
              activeProfile={activeSftpProfile}
              onOpenFile={onOpenSftpFile}
            />
          )}
        </div>
        <div className="files-preview aurora-scrollbar">
          {!pane.selected ? (
            <div className="files-empty operation-muted">Select a file</div>
          ) : pane.file.kind === "loading" ? (
            <div className="files-empty">
              <Loader2 size={16} className="files-spin" />
              <span>Loading...</span>
            </div>
          ) : pane.file.kind === "error" ? (
            <div className="files-empty operation-muted">{pane.file.message}</div>
          ) : pane.file.kind === "loaded" ? (
            <FilePreview
              selectedPath={pane.selected.path}
              modifiedUnix={pane.selected.modifiedUnix}
              file={pane.file.value}
              editing={pane.editing}
              draft={pane.draft}
              saving={pane.saving}
              indexState={indexState}
              canIndex={Boolean(client && config) && isIndexable(pane.selected.name)}
              canEdit={pane.selected.origin !== "sftp"}
              onEdit={() => onSetEditing(true)}
              onCancelEdit={onCancelEdit}
              onDraftChange={onDraftChange}
              onSave={onSave}
              onIndex={onIndex}
              sparkleOpen={pane.sparkleOpen}
              sparkleQuery={pane.sparkleQuery}
              proposal={pane.proposal}
              proposalState={pane.proposalState}
              proposalErrorMessage={pane.proposalErrorMessage}
              onSparkleToggle={onSparkleToggle}
              onSparkleQueryChange={onSparkleQueryChange}
              onSparkleSubmit={onSparkleSubmit}
              onProposalDeny={onProposalDeny}
              onProposalApprove={onProposalApprove}
            />
          ) : null}
        </div>
      </div>
    </div>
  );
}

function FilePreview({
  selectedPath,
  modifiedUnix,
  file,
  editing,
  draft,
  saving,
  indexState,
  canIndex,
  canEdit,
  onEdit,
  onCancelEdit,
  onDraftChange,
  onSave,
  onIndex,
  sparkleOpen,
  sparkleQuery,
  proposal,
  proposalState,
  proposalErrorMessage,
  onSparkleToggle,
  onSparkleQueryChange,
  onSparkleSubmit,
  onProposalDeny,
  onProposalApprove,
}: {
  selectedPath: string;
  modifiedUnix?: number | null;
  file: FileContents;
  editing: boolean;
  draft: string;
  saving: boolean;
  indexState: IndexState;
  canIndex: boolean;
  /** SFTP is v1 read-only browsing: both the manual Edit button and the
   * "Edit with the model" sparkle button are hard-disabled (not rendered)
   * for any file whose pane resolves to an SFTP-origin entry. */
  canEdit: boolean;
  onEdit: () => void;
  onCancelEdit: () => void;
  onDraftChange: (value: string) => void;
  onSave: () => void;
  onIndex: () => void;
  sparkleOpen: boolean;
  sparkleQuery: string;
  proposal: FilesPane["proposal"];
  proposalState: FilesPane["proposalState"];
  proposalErrorMessage: string | null;
  onSparkleToggle: () => void;
  onSparkleQueryChange: (value: string) => void;
  onSparkleSubmit: () => void;
  onProposalDeny: () => void;
  onProposalApprove: () => void;
}) {
  const name = selectedPath.split("/").pop() ?? selectedPath;
  return (
    <div className="files-preview-inner">
      <div className="files-preview-header">
        <span className="files-preview-name">{name}</span>
        <span className="files-preview-meta">
          {formatBytes(file.size)} · modified {formatModified(modifiedUnix)}
        </span>
        <div className="files-preview-actions">
          {editing ? (
            <>
              <Button variant="ghost" size="sm" type="button" onClick={onCancelEdit}>
                Cancel
              </Button>
              <Button variant="aurora" size="sm" type="button" onClick={onSave} disabled={saving}>
                <Save size={13} />
                {saving ? "Saving..." : "Save"}
              </Button>
            </>
          ) : (
            <>
              {canEdit && (
                <Button variant="ghost" size="sm" type="button" onClick={onEdit}>
                  Edit
                </Button>
              )}
              {canEdit && (
                <Button
                  variant="plain"
                  size="unstyled"
                  type="button"
                  title="Edit with the model"
                  aria-label="Edit with the model"
                  onClick={onSparkleToggle}
                >
                  <Sparkles size={14} />
                </Button>
              )}
              {canIndex && (
                <Button
                  variant="aurora"
                  size="sm"
                  type="button"
                  onClick={onIndex}
                  disabled={indexState.kind === "running"}
                >
                  <Upload size={13} />
                  {indexState.kind === "running" ? "Indexing..." : "Index"}
                </Button>
              )}
            </>
          )}
        </div>
      </div>
      {indexState.kind === "done" && (
        <div className={`files-index-status${indexState.ok ? "" : " files-index-status-error"}`}>
          {indexState.message}
        </div>
      )}
      {editing ? (
        <textarea
          className="files-editor"
          value={draft}
          onChange={(event) => onDraftChange(event.target.value)}
          spellCheck={false}
        />
      ) : isMarkdownLike(name) ? (
        <pre className="files-preview-text">{file.content}</pre>
      ) : (
        <pre className="files-preview-text files-preview-code">
          <code>{file.content}</code>
        </pre>
      )}
      <AiEditPanel
        sparkleOpen={sparkleOpen}
        sparkleQuery={sparkleQuery}
        proposal={proposal}
        proposalState={proposalState}
        proposalErrorMessage={proposalErrorMessage}
        onSparkleToggle={onSparkleToggle}
        onSparkleQueryChange={onSparkleQueryChange}
        onSparkleSubmit={onSparkleSubmit}
        onProposalDeny={onProposalDeny}
        onProposalApprove={onProposalApprove}
      />
    </div>
  );
}
