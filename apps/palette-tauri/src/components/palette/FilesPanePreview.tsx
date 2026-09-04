import { FileSearch, Loader2 } from "lucide-react";

import { formatBytes, isIndexable } from "@/lib/filesModel";
import { FilePreview } from "./FilePreview";
import type { FilesPaneViewProps } from "./filesPaneTypes";

type FilesPanePreviewProps = Pick<
  FilesPaneViewProps,
  | "pane"
  | "listing"
  | "indexState"
  | "client"
  | "config"
  | "onSetEditing"
  | "onCancelEdit"
  | "onDraftChange"
  | "onSave"
  | "onIndex"
  | "onSparkleToggle"
  | "onSparkleQueryChange"
  | "onSparkleSubmit"
  | "onProposalDeny"
  | "onProposalApprove"
> & {
  directoryStats: { files: number; folders: number; indexable: number; size: number };
};

export function FilesPanePreview({
  pane,
  listing,
  indexState,
  client,
  config,
  directoryStats,
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
}: FilesPanePreviewProps) {
  if (!pane.selected) {
    return (
      <div className="files-welcome">
        <span className="sr-only">Select a file</span>
        <div className="files-welcome-icon">
          <FileSearch size={20} />
        </div>
        <h3>Select a file to inspect</h3>
        <p>Preview, edit, or index content from this folder.</p>
        <div className="files-directory-summary" role="group" aria-label="Current folder summary">
          <span>
            <strong>{directoryStats.files}</strong> files
          </span>
          <span>
            <strong>{directoryStats.folders}</strong> folders
          </span>
          <span>
            <strong>{directoryStats.indexable}</strong> indexable
          </span>
          <span>
            <strong>{formatBytes(directoryStats.size)}</strong> total
          </span>
        </div>
      </div>
    );
  }
  if (pane.file.kind === "loading") {
    return (
      <div className="files-empty">
        <Loader2 size={16} className="files-spin" />
        <span>Loading...</span>
      </div>
    );
  }
  if (pane.file.kind === "error") {
    return <div className="files-empty operation-muted">{pane.file.message}</div>;
  }
  if (pane.file.kind !== "loaded") return null;
  return (
    <FilePreview
      selectedPath={pane.selected.path}
      absolutePath={
        pane.selected.origin === "sftp" || listing.kind !== "loaded"
          ? null
          : `${listing.value.root.replace(/\/$/, "")}/${pane.selected.path}`
      }
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
  );
}
