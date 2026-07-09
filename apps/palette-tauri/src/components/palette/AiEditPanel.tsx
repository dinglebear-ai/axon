import { Sparkles } from "lucide-react";

import { Button } from "@/components/ui/aurora/button";
import type { AiEditProposal } from "@/lib/aiEditModel";
import type { FilesPane } from "@/lib/filesModel";

/** Cap on rendered AI-edit diff lines — see the render site's comment (P2 #9). */
const MAX_RENDERED_DIFF_LINES = 500;

/**
 * The AI-assisted edit UI: either a proposed-diff review (with Deny/Approve)
 * when a proposal is ready, or the inline "Edit with the model" instruction
 * prompt when the user has opened the sparkle input. Renders nothing when
 * neither is active. Pure presentational — all state transitions are
 * dispatched by the parent via the callback props.
 */
export function AiEditPanel({
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
  sparkleOpen: boolean;
  sparkleQuery: string;
  proposal: AiEditProposal | null;
  proposalState: FilesPane["proposalState"];
  proposalErrorMessage: string | null;
  onSparkleToggle: () => void;
  onSparkleQueryChange: (value: string) => void;
  onSparkleSubmit: () => void;
  onProposalDeny: () => void;
  onProposalApprove: () => void;
}) {
  if (proposal) {
    return (
      <div className="files-ai-edit-review">
        <p className="files-ai-edit-heading">
          Proposed edit · {proposal.diff.filter((l) => l.kind !== "same").length} lines
        </p>
        <pre className="files-ai-edit-body">
          {/* Diff lines have no stable identity of their own — line text can
              repeat, and the rendered order never reorders after the
              proposal is set — so an index key is safe here. */}
          {/* computeLineDiff is index-aligned, not LCS: a single top-of-file
              insertion cascades into marking the entire remainder of the
              file as removed+added. Without a cap, a several-thousand-line
              file renders one <div> per line with no virtualization and
              visibly janks. Capping the rendered count (not a full
              virtualization rewrite) keeps this proportionate — see P2 #9. */}
          {proposal.diff.slice(0, MAX_RENDERED_DIFF_LINES).map((line, index) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: see comment above.
            <div key={index} className={`files-ai-edit-line files-ai-edit-${line.kind}`}>
              <span className="files-ai-edit-marker">
                {line.kind === "added" ? "+" : line.kind === "removed" ? "-" : " "}
              </span>
              {line.text}
            </div>
          ))}
          {proposal.diff.length > MAX_RENDERED_DIFF_LINES && (
            <div className="files-ai-edit-line files-ai-edit-truncated operation-muted">
              … {proposal.diff.length - MAX_RENDERED_DIFF_LINES} more lines not shown
            </div>
          )}
        </pre>
        {proposalState === "error" && proposalErrorMessage && (
          <p className="files-ai-edit-error">{proposalErrorMessage}</p>
        )}
        <div className="files-ai-edit-actions">
          <span className="files-ai-edit-note">The model proposes this edit — review it.</span>
          <Button variant="ghost" size="sm" type="button" onClick={onProposalDeny}>
            Deny
          </Button>
          <Button
            variant="rose"
            size="sm"
            type="button"
            onClick={onProposalApprove}
            disabled={proposalState === "approving"}
          >
            {proposalState === "approving" ? "Applying..." : "Approve"}
          </Button>
        </div>
      </div>
    );
  }

  if (!sparkleOpen) return null;

  return (
    <div className="files-ai-edit-prompt">
      <Sparkles size={14} />
      {/* Autofocus is intentional here: this input only mounts when the
          user explicitly clicks "Edit with the model", so focusing it
          immediately (like a command-palette input opening) is expected,
          not a surprise page-load autofocus. */}
      <input
        // biome-ignore lint/a11y/noAutofocus: see comment above the input.
        autoFocus
        value={sparkleQuery}
        placeholder="Describe the edit — the model rewrites the file…"
        onChange={(event) => onSparkleQueryChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && sparkleQuery.trim()) onSparkleSubmit();
          if (event.key === "Escape") onSparkleToggle();
        }}
      />
      <Button
        variant="rose"
        size="icon"
        title="Generate edit"
        aria-label="Generate edit"
        type="button"
        onClick={onSparkleSubmit}
        disabled={proposalState === "pending"}
      >
        <Sparkles size={14} />
      </Button>
      {proposalState === "error" && proposalErrorMessage && (
        <p className="files-ai-edit-error">{proposalErrorMessage}</p>
      )}
    </div>
  );
}
