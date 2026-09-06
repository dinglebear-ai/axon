import type { FileContents, FilesPane } from "@/lib/filesModel";
import type { IndexState } from "./FilesPaneView";

export interface FilePreviewProps {
  selectedPath: string;
  absolutePath: string | null;
  modifiedUnix?: number | null;
  file: FileContents;
  editing: boolean;
  draft: string;
  saving: boolean;
  indexState: IndexState;
  canIndex: boolean;
  /** SFTP is read-only, so neither editing entry point is rendered. */
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
}

export const SYNTAX_LANGUAGES = [
  "text",
  "html",
  "css",
  "javascript",
  "typescript",
  "tsx",
  "json",
  "markdown",
  "bash",
  "rust",
  "python",
  "sql",
  "yaml",
  "toml",
  "xml",
  "diff",
];
