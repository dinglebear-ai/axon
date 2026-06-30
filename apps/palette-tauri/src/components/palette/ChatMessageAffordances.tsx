import { Copy, Pencil, RotateCcw, Sparkles } from "lucide-react";

import { MessageActionButton } from "@/components/aurora/ai/message";
import { Source } from "@/components/aurora/ai/source";
import type { AskTurn, ChatSuggestion } from "@/lib/runState";

export type SuggestionState =
  | { status: "loading" }
  | { status: "ready"; rows: ChatSuggestion[] }
  | { status: "error"; message: string };

export function ChatMessageActions({
  enabled,
  turn,
  suggestion,
  onSuggest,
  onEdit,
  onRegenerate,
}: {
  enabled: boolean;
  turn: AskTurn;
  suggestion?: SuggestionState;
  onSuggest?: (turn: AskTurn) => void;
  onEdit?: (turn: AskTurn) => void;
  onRegenerate?: (turn: AskTurn) => void;
}) {
  if (turn.pending || !turn.content.trim()) return null;
  const loading = suggestion?.status === "loading";
  return (
    <div className="chat-message-tools">
      <MessageActionButton
        onClick={() => void navigator.clipboard?.writeText(turn.content).catch(() => {})}
        aria-label={`Copy ${turn.role} message`}
        title="Copy"
      >
        <Copy size={13} strokeWidth={1.8} aria-hidden="true" />
      </MessageActionButton>
      {onEdit ? (
        <MessageActionButton onClick={() => onEdit(turn)} aria-label={`Edit ${turn.role} message`} title="Edit">
          <Pencil size={13} strokeWidth={1.8} aria-hidden="true" />
        </MessageActionButton>
      ) : null}
      {onRegenerate ? (
        <MessageActionButton onClick={() => onRegenerate(turn)} aria-label={`Regenerate from ${turn.role} message`} title="Regenerate">
          <RotateCcw size={13} strokeWidth={1.8} aria-hidden="true" />
        </MessageActionButton>
      ) : null}
      {enabled && onSuggest ? (
        <MessageActionButton
          className="chat-message-tool"
          onClick={() => onSuggest(turn)}
          disabled={loading}
          aria-label={`Suggest docs for ${turn.role} message`}
          title={loading ? "Searching indexed docs" : "Suggest relevant docs"}
        >
          <Sparkles size={13} strokeWidth={1.8} aria-hidden="true" />
        </MessageActionButton>
      ) : null}
    </div>
  );
}

export function ChatSuggestionPanel({ align = "start", suggestion }: { align?: "start" | "end"; suggestion?: SuggestionState }) {
  if (!suggestion) return null;
  if (suggestion.status === "loading") {
    return (
      <div className={`chat-suggestion-panel chat-suggestion-panel-${align}`} role="status">
        Searching indexed docs...
      </div>
    );
  }
  if (suggestion.status === "error") {
    return (
      <div className={`chat-suggestion-panel chat-suggestion-panel-${align} chat-suggestion-error`} role="alert">
        {suggestion.message}
      </div>
    );
  }
  if (suggestion.rows.length === 0) {
    return <div className={`chat-suggestion-panel chat-suggestion-panel-${align}`}>No indexed docs matched this message.</div>;
  }
  return (
    <section className={`chat-suggestion-panel chat-suggestion-panel-${align}`} aria-label="Suggested docs">
      {suggestion.rows.map((row) => (
        <Source
          key={`${row.url ?? row.title}-${row.rank}`}
          className="chat-suggestion-row"
          source={{ title: row.title, href: row.url, description: row.snippet, badge: row.score !== undefined ? row.score.toFixed(3) : undefined }}
          index={row.rank}
        />
      ))}
    </section>
  );
}
