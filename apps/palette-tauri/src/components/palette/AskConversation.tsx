import { Download, GitBranch, Layers3, Paperclip, Quote, Send, X } from "lucide-react";
import { Fragment, memo, useEffect, useMemo, useState } from "react";

import { actionIcon } from "@/components/palette/ActionIcon";
import type { SuggestionState } from "@/components/palette/ChatMessageAffordances";
import { ConversationThread } from "@/components/palette/ConversationThread";
import { Button } from "@/components/ui/aurora/button";
import { Input } from "@/components/ui/aurora/input";
import { actionDisplayMeta } from "@/lib/actionMeta";
import { ACTIONS, type PaletteAction } from "@/lib/actions";
import type { Client, PaletteConfig } from "@/lib/axonClient";
import { sortActionsByRelevance } from "@/lib/paletteView";
import type { AskTurn, ChatSuggestion } from "@/lib/runState";

export { ConversationThread } from "@/components/palette/ConversationThread";

// The full ask view: a conversation thread plus a follow-up compose box.
export const AskConversation = memo(function AskConversation({
  prompt,
  answer,
  transcript,
  pending,
  onFollowUp,
  onRunAction,
  suggestionsEnabled = false,
  onSuggestMessage,
  agentBubbles = false,
  client = null,
  config = null,
}: {
  prompt?: string;
  answer?: string;
  transcript?: AskTurn[];
  pending?: boolean;
  onFollowUp: (text: string) => void;
  onRunAction?: (subcommand: string, argument: string) => void;
  suggestionsEnabled?: boolean;
  onSuggestMessage?: (message: string) => Promise<ChatSuggestion[]>;
  agentBubbles?: boolean;
  client?: Client | null;
  config?: PaletteConfig | null;
  action?: "ask" | "chat";
}) {
  const [draft, setDraft] = useState("");
  const [selectedCommand, setSelectedCommand] = useState(0);
  const [selectedSlashAction, setSelectedSlashAction] = useState<PaletteAction | null>(null);
  const [suggestionsByTurn, setSuggestionsByTurn] = useState<Record<string, SuggestionState>>({});
  const [contextOpen, setContextOpen] = useState(false);
  const [branches, setBranches] = useState<Array<{ id: string; label: string; turns: AskTurn[] }>>(
    [],
  );
  const [activeBranch, setActiveBranch] = useState("main");
  const canSend = draft.trim().length > 0 && !pending;
  const visibleTranscript =
    activeBranch === "main"
      ? transcript
      : (branches.find((branch) => branch.id === activeBranch)?.turns ?? transcript);
  const contextSources = useMemo(
    () => visibleTranscript?.flatMap((turn) => turn.sources ?? []) ?? [],
    [visibleTranscript],
  );
  const activityCount = useMemo(
    () =>
      visibleTranscript?.reduce((total, turn) => total + (turn.activities?.length ?? 0), 0) ?? 0,
    [visibleTranscript],
  );
  const slashQuery =
    !selectedSlashAction && draft.startsWith("/") ? draft.slice(1).trimStart() : null;
  const slashMenuOpen = slashQuery !== null && !pending && Boolean(onRunAction);
  const slashCommands = useMemo(() => {
    if (slashQuery === null) return [];
    const needle = slashQuery.split(/\s+/, 1)[0]?.toLowerCase() ?? "";
    return sortActionsByRelevance(
      ACTIONS.filter((action) => {
        if (action.subcommand === "chat" || action.subcommand === "ask") return false;
        if (!needle) return true;
        const meta = actionDisplayMeta(action);
        return (
          action.subcommand.toLowerCase().includes(needle) ||
          action.label.toLowerCase().includes(needle) ||
          meta.label.toLowerCase().includes(needle) ||
          action.description.toLowerCase().includes(needle) ||
          action.aliases.some((alias) => alias.toLowerCase().includes(needle))
        );
      }),
      needle,
    ).slice(0, 10);
  }, [slashQuery]);
  const clampedSelectedCommand = Math.min(selectedCommand, Math.max(slashCommands.length - 1, 0));
  const slashCommandGroups = useMemo(() => {
    const groups: { category: string; actions: PaletteAction[] }[] = [];
    for (const action of slashCommands) {
      const category = actionDisplayMeta(action).category;
      const group = groups.find((candidate) => candidate.category === category);
      if (group) group.actions.push(action);
      else groups.push({ category, actions: [action] });
    }
    return groups;
  }, [slashCommands]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset the highlighted slash command whenever the typed query changes.
  useEffect(() => {
    setSelectedCommand(0);
  }, [slashQuery]);

  useEffect(() => {
    if (activeBranch === "main" || !transcript) return;
    setBranches((current) =>
      current.map((branch) =>
        branch.id === activeBranch ? { ...branch, turns: [...transcript] } : branch,
      ),
    );
  }, [activeBranch, transcript]);

  function resetSlashAction() {
    setSelectedSlashAction(null);
    setDraft("");
  }

  function selectSlashCommand(action: PaletteAction, argument: string) {
    if (action.argMode === "none") {
      runSlashCommand(action, "");
      return;
    }
    setSelectedSlashAction(action);
    setDraft(argument.trimStart());
  }

  function runSlashCommand(action: PaletteAction, argument: string) {
    if (!onRunAction) return;
    if (action.argMode !== "none" && !argument.trim()) {
      selectSlashCommand(action, "");
      return;
    }
    setSelectedSlashAction(null);
    setDraft("");
    onRunAction(action.subcommand, argument.trim());
  }

  function submitDraft() {
    const value = draft.trim();
    if (!value || pending) return;
    if (selectedSlashAction) {
      runSlashCommand(selectedSlashAction, value);
      return;
    }
    if (value.startsWith("/") && onRunAction) {
      const [token = "", ...rest] = value.slice(1).trim().split(/\s+/);
      const normalizedToken = token.toLowerCase();
      const action = ACTIONS.find(
        (candidate) =>
          candidate.subcommand === normalizedToken ||
          candidate.aliases.some((alias) => alias.toLowerCase() === normalizedToken),
      );
      if (action && action.subcommand !== "ask" && action.subcommand !== "chat") {
        if (rest.length === 0 && action.argMode !== "none") {
          selectSlashCommand(action, "");
        } else {
          runSlashCommand(action, rest.join(" "));
        }
        return;
      }
    }
    setDraft("");
    onFollowUp(value);
  }

  async function suggestTurn(turn: AskTurn) {
    if (!onSuggestMessage || !turn.content.trim()) return;
    setSuggestionsByTurn((current) => ({ ...current, [turn.id]: { status: "loading" } }));
    try {
      const rows = await onSuggestMessage(turn.content);
      setSuggestionsByTurn((current) => ({ ...current, [turn.id]: { status: "ready", rows } }));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setSuggestionsByTurn((current) => ({ ...current, [turn.id]: { status: "error", message } }));
    }
  }

  function editTurn(turn: AskTurn) {
    setSelectedSlashAction(null);
    setDraft(turn.content);
  }

  function regenerateTurn(turn: AskTurn) {
    const value = turn.content.trim();
    if (!value || pending) return;
    setDraft("");
    onFollowUp(value);
  }

  function downloadConversation() {
    const markdown = (visibleTranscript ?? [])
      .map((turn) => `## ${turn.role === "user" ? "You" : "Axon"}\n\n${turn.content}`)
      .join("\n\n---\n\n");
    const url = URL.createObjectURL(new Blob([markdown], { type: "text/markdown" }));
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `axon-conversation-${new Date().toISOString().slice(0, 10)}.md`;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  function forkConversation() {
    const id = `branch-${branches.length + 1}`;
    setBranches((current) => [
      ...current,
      { id, label: `Branch ${current.length + 1}`, turns: [...(visibleTranscript ?? [])] },
    ]);
    setActiveBranch(id);
  }

  function quoteSelection() {
    const selection = window.getSelection()?.toString().trim();
    if (!selection) return;
    setDraft(
      (current) => `${current}${current ? "\n\n" : ""}> ${selection.replace(/\n/g, "\n> ")}\n\n`,
    );
  }

  return (
    <div className="ask-body">
      <div className="ask-workspace-toolbar">
        <div className="ask-branch-control">
          <GitBranch size={13} />
          <select
            value={activeBranch}
            onChange={(event) => setActiveBranch(event.target.value)}
            aria-label="Conversation branch"
          >
            <option value="main">Main</option>
            {branches.map((branch) => (
              <option key={branch.id} value={branch.id}>
                {branch.label}
              </option>
            ))}
          </select>
          <button type="button" onClick={forkConversation}>
            Branch
          </button>
        </div>
        <button
          type="button"
          onClick={() => setContextOpen((value) => !value)}
          className={contextOpen ? "is-active" : ""}
        >
          <Layers3 size={13} /> Context <span>{contextSources.length}</span>
        </button>
        <button type="button" onClick={quoteSelection} title="Quote selected answer text">
          <Quote size={13} /> Quote
        </button>
        <button
          type="button"
          onClick={downloadConversation}
          disabled={!visibleTranscript?.length}
          title="Save conversation as Markdown"
        >
          <Download size={13} /> Save
        </button>
      </div>
      {contextOpen ? (
        <aside className="ask-context-panel">
          <header>
            <strong>Context used</strong>
            <span>
              {contextSources.length} sources · {activityCount} activity steps
            </span>
          </header>
          {contextSources.length ? (
            <div>
              {contextSources.map((source) => (
                <span key={`${source.url ?? source.label}:${source.title ?? "source"}`}>
                  {source.title ?? source.label}
                </span>
              ))}
            </div>
          ) : (
            <p>No retrieval sources were attached to this answer.</p>
          )}
        </aside>
      ) : null}
      <ConversationThread
        prompt={prompt}
        answer={answer ?? ""}
        turns={visibleTranscript}
        suggestionsEnabled={suggestionsEnabled && Boolean(onSuggestMessage)}
        suggestionsByTurn={suggestionsByTurn}
        onSuggestTurn={suggestTurn}
        onEditTurn={editTurn}
        onRegenerateTurn={regenerateTurn}
        agentBubbles={agentBubbles}
        client={client}
        config={config}
      />
      <form
        className="ask-compose"
        onSubmit={(event) => {
          event.preventDefault();
          submitDraft();
        }}
      >
        {slashMenuOpen && slashCommands.length > 0 ? (
          <div className="ask-slash-menu" role="listbox" aria-label="Palette commands">
            {slashCommandGroups.map((group) => (
              <Fragment key={group.category}>
                <div className="ask-slash-heading">{group.category}</div>
                {group.actions.map((action) => {
                  const Icon = actionIcon(action.subcommand);
                  const meta = actionDisplayMeta(action);
                  const index = slashCommands.findIndex(
                    (candidate) => candidate.subcommand === action.subcommand,
                  );
                  const selected = index === clampedSelectedCommand;
                  return (
                    <Button
                      variant="plain"
                      size="unstyled"
                      className={`ask-slash-option${selected ? " ask-slash-option-selected" : ""}`}
                      type="button"
                      role="option"
                      aria-selected={selected}
                      key={action.subcommand}
                      onMouseEnter={() => setSelectedCommand(index)}
                      onClick={() => {
                        const argument = draft.slice(1).trim().split(/\s+/).slice(1).join(" ");
                        selectSlashCommand(action, argument);
                      }}
                    >
                      <Icon size={15} strokeWidth={1.8} aria-hidden="true" />
                      <span>
                        <strong>/{action.subcommand}</strong>
                        <small>
                          {meta.input === "none" ? meta.output : `${meta.input} -> ${meta.output}`}
                        </small>
                      </span>
                      <em>{meta.method}</em>
                    </Button>
                  );
                })}
              </Fragment>
            ))}
          </div>
        ) : null}
        <Button
          variant="plain"
          size="unstyled"
          className="ask-attach"
          type="button"
          disabled
          aria-label="Attach context"
          title="Attach context"
          aria-disabled="true"
        >
          <Paperclip size={18} strokeWidth={1.75} />
        </Button>
        <div className="ask-compose-input">
          {selectedSlashAction ? (
            <Button
              variant="plain"
              size="unstyled"
              className={`ask-action-chip ask-action-chip-${selectedSlashAction.tone}`}
              type="button"
              onClick={resetSlashAction}
              aria-label={`Clear ${actionDisplayMeta(selectedSlashAction).label} selection`}
              title={`/${selectedSlashAction.subcommand}`}
            >
              {(() => {
                const Icon = actionIcon(selectedSlashAction.subcommand);
                return <Icon size={14} strokeWidth={1.85} aria-hidden="true" />;
              })()}
              <span>{actionDisplayMeta(selectedSlashAction).label}</span>
              <X size={12} strokeWidth={1.9} aria-hidden="true" />
            </Button>
          ) : null}
          <Input
            unstyled
            value={draft}
            disabled={pending}
            onChange={(event) => {
              setDraft(event.target.value);
              if (selectedSlashAction && event.target.value.startsWith("/"))
                setSelectedSlashAction(null);
            }}
            onKeyDown={(event) => {
              if (selectedSlashAction && event.key === "Escape") {
                event.preventDefault();
                resetSlashAction();
                return;
              }
              if (!slashMenuOpen || slashCommands.length === 0) return;
              if (event.key === "ArrowDown") {
                event.preventDefault();
                setSelectedCommand((index) => Math.min(index + 1, slashCommands.length - 1));
              } else if (event.key === "ArrowUp") {
                event.preventDefault();
                setSelectedCommand((index) => Math.max(index - 1, 0));
              } else if (event.key === "Tab") {
                event.preventDefault();
                const argument = draft.slice(1).trim().split(/\s+/).slice(1).join(" ");
                selectSlashCommand(slashCommands[clampedSelectedCommand], argument);
              } else if (event.key === "Enter") {
                event.preventDefault();
                const argument = draft.slice(1).trim().split(/\s+/).slice(1).join(" ");
                if (argument.trim())
                  runSlashCommand(slashCommands[clampedSelectedCommand], argument);
                else selectSlashCommand(slashCommands[clampedSelectedCommand], argument);
              } else if (event.key === "Escape") {
                event.preventDefault();
                setDraft("");
              }
            }}
            placeholder={
              pending
                ? "Waiting for response..."
                : selectedSlashAction
                  ? selectedSlashAction.example.replace(
                      new RegExp(`^${selectedSlashAction.subcommand}\\s*`, "i"),
                      "",
                    )
                  : "Ask a follow-up..."
            }
            aria-label="Ask a follow-up"
          />
        </div>
        {/* Scoped by `.ask-compose button`. type="submit" MUST be explicit — the
            Button primitive never defaults it, and this is the app's only submit
            button, so Enter-to-send would break silently without it. */}
        <Button
          variant="plain"
          size="unstyled"
          className={`command-submit command-submit-rose${canSend ? " command-submit-armed" : ""} disabled:opacity-100`}
          type="submit"
          disabled={!canSend}
          aria-label="Send follow-up"
          title="Send follow-up"
        >
          <Send size={15} />
        </Button>
      </form>
    </div>
  );
});
