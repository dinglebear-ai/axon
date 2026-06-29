import { memo, useEffect, useMemo, useRef, useState } from "react";
import { Brain, CheckCircle2, Paperclip, Send, Wrench } from "lucide-react";

import { actionIcon } from "@/components/palette/ActionIcon";
import { Button } from "@/components/ui/aurora/button";
import { Input } from "@/components/ui/aurora/input";
import { AxonMark } from "@/components/palette/AxonMark";
import { MarkdownBody } from "@/components/palette/MarkdownBody";
import { ACTIONS, type PaletteAction } from "@/lib/actions";
import { actionDisplayMeta } from "@/lib/actionMeta";
import type { AskActivity, AskSource, AskTurn } from "@/lib/runState";

function hasRichMarkdown(value: string): boolean {
  return /```|^\s{0,3}#{1,6}\s|^\s{0,3}(?:[-*+]|\d+\.)\s|^\s{0,3}>\s|\n\|.+\|\n|\[[^\]]+\]\([^)]+\)/m.test(value);
}

function AskAnswerBody({ answer }: { answer: string }) {
  if (hasRichMarkdown(answer)) {
    return <MarkdownBody>{answer}</MarkdownBody>;
  }

  return (
    <div className="ask-answer-prose">
      {answer.split(/\n{2,}/).map((paragraph, index) => {
        const lines = paragraph.split("\n");
        return (
          <p key={`${index}-${paragraph.slice(0, 16)}`}>
            {lines.map((line, lineIndex) => (
              <span key={`${lineIndex}-${line.slice(0, 16)}`}>
                {line}
                {lineIndex < lines.length - 1 ? <br /> : null}
              </span>
            ))}
          </p>
        );
      })}
    </div>
  );
}

function SourceStrip({ sources }: { sources?: AskSource[] }) {
  if (!sources?.length) return null;
  return (
    <details className="ask-sources">
      <summary>Sources</summary>
      <div>
        {sources.map((source, index) => (
          source.url ? (
            <a key={`${source.url}-${index}`} href={source.url} target="_blank" rel="noreferrer">
              <span>{index + 1}</span>
              {source.label}
            </a>
          ) : (
            <span key={`${source.label}-${index}`}>
              <span>{index + 1}</span>
              {source.label}
            </span>
          )
        ))}
      </div>
    </details>
  );
}

function ActivityIcon({ activity }: { activity: AskActivity }) {
  if (activity.kind === "tool") return <Wrench size={12} strokeWidth={1.8} aria-hidden="true" />;
  if (activity.kind === "done") return <CheckCircle2 size={12} strokeWidth={1.8} aria-hidden="true" />;
  return <Brain size={12} strokeWidth={1.8} aria-hidden="true" />;
}

function ActivityTrail({ activities, pending }: { activities?: AskActivity[]; pending?: boolean }) {
  if (!activities?.length) return null;
  return (
    <section className="ask-activity" aria-label={pending ? "Agent activity" : "Agent activity summary"}>
      {activities.map((activity) => (
        <div key={activity.id} className={`ask-activity-row ask-activity-${activity.kind ?? "thinking"}`}>
          <ActivityIcon activity={activity} />
          <span>
            <strong>{activity.label}</strong>
            {activity.detail ? <small>{activity.detail}</small> : null}
          </span>
        </div>
      ))}
    </section>
  );
}

// A read-only question→answer pair rendered with the ask bubble styling. Reused
// by the live ask view and the side-by-side evaluate view. Memoized so unrelated
// App re-renders during a stream don't recompute the thread (P-M1).
export const ConversationThread = memo(function ConversationThread({
  prompt,
  answer,
  turns,
  waiting = "Waiting for response...",
  reader = false,
}: {
  prompt?: string;
  answer: string;
  turns?: AskTurn[];
  waiting?: string;
  reader?: boolean;
}) {
  const threadTurns = useMemo<AskTurn[]>(
    () =>
      turns?.length
        ? turns
        : [
            ...(prompt ? [{ id: "legacy:user", role: "user" as const, content: prompt }] : []),
            { id: "legacy:assistant", role: "assistant" as const, content: answer },
          ],
    [answer, prompt, turns],
  );
  const threadRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);
  const signature = threadTurns.map((turn) => `${turn.id}:${turn.content.length}:${turn.pending ? "pending" : "done"}`).join("|");

  useEffect(() => {
    const element = threadRef.current;
    if (!element || !stickToBottom.current) return;
    element.scrollTop = element.scrollHeight;
  }, [signature]);

  function onThreadScroll() {
    const element = threadRef.current;
    if (!element) return;
    const distanceFromBottom = element.scrollHeight - element.clientHeight - element.scrollTop;
    stickToBottom.current = distanceFromBottom < 36;
  }

  if (reader) {
    return (
      <div className="ask-thread ask-thread-reader aurora-scrollbar">
        {prompt ? (
          <div className="ask-prompt-strip">
            <span>Question</span>
            <p>{prompt}</p>
          </div>
        ) : null}
        <div className="ask-answer ask-answer-reader">
          {answer ? <MarkdownBody>{answer}</MarkdownBody> : <span className="ask-waiting">{waiting}</span>}
        </div>
      </div>
    );
  }

  return (
    <div ref={threadRef} className="ask-thread aurora-scrollbar" role="group" aria-label="Ask conversation" onScroll={onThreadScroll}>
      {threadTurns.map((turn) =>
        turn.role === "user" ? (
          <div key={turn.id} className="ask-message ask-message-user">
            <span>You</span>
            <p>{turn.content}</p>
          </div>
        ) : (
          <div key={turn.id} className="ask-message ask-message-assistant">
            <span className="ask-assistant-avatar" aria-label="Axon">
              <AxonMark size={18} />
            </span>
            <ActivityTrail activities={turn.activities} pending={turn.pending} />
            <div className="ask-answer">
              {turn.content ? <AskAnswerBody answer={turn.content} /> : <span className="ask-waiting">{waiting}</span>}
            </div>
            <SourceStrip sources={turn.sources} />
          </div>
        ),
      )}
    </div>
  );
});

// The full ask view: a conversation thread plus a follow-up compose box.
export const AskConversation = memo(function AskConversation({
  prompt,
  answer,
  transcript,
  pending,
  onFollowUp,
  onRunAction,
}: {
  prompt?: string;
  answer?: string;
  transcript?: AskTurn[];
  pending?: boolean;
  onFollowUp: (text: string) => void;
  onRunAction?: (subcommand: string, argument: string) => void;
}) {
  const [draft, setDraft] = useState("");
  const [selectedCommand, setSelectedCommand] = useState(0);
  const canSend = draft.trim().length > 0 && !pending;
  const slashQuery = draft.startsWith("/") ? draft.slice(1).trimStart() : null;
  const slashMenuOpen = slashQuery !== null && !pending && Boolean(onRunAction);
  const slashCommands = useMemo(() => {
    if (slashQuery === null) return [];
    const needle = slashQuery.split(/\s+/, 1)[0]?.toLowerCase() ?? "";
    return ACTIONS.filter((action) => {
      if (action.subcommand === "chat" || action.subcommand === "ask") return false;
      if (!needle) return true;
      const meta = actionDisplayMeta(action);
      return (
        action.subcommand.toLowerCase().includes(needle) ||
        action.label.toLowerCase().includes(needle) ||
        meta.label.toLowerCase().includes(needle) ||
        action.aliases.some((alias) => alias.toLowerCase().includes(needle))
      );
    }).slice(0, 10);
  }, [slashQuery]);
  const clampedSelectedCommand = Math.min(selectedCommand, Math.max(slashCommands.length - 1, 0));

  useEffect(() => {
    setSelectedCommand(0);
  }, [slashQuery]);

  function runSlashCommand(action: PaletteAction, argument: string) {
    if (!onRunAction) return;
    if (action.argMode !== "none" && !argument.trim()) {
      setDraft(`/${action.subcommand} `);
      return;
    }
    setDraft("");
    onRunAction(action.subcommand, argument.trim());
  }

  function submitDraft() {
    const value = draft.trim();
    if (!value || pending) return;
    if (value.startsWith("/") && onRunAction) {
      const [token = "", ...rest] = value.slice(1).trim().split(/\s+/);
      const normalizedToken = token.toLowerCase();
      const action = ACTIONS.find(
        (candidate) => candidate.subcommand === normalizedToken || candidate.aliases.some((alias) => alias.toLowerCase() === normalizedToken),
      );
      if (action && action.subcommand !== "ask" && action.subcommand !== "chat") {
        runSlashCommand(action, rest.join(" "));
        return;
      }
    }
    setDraft("");
    onFollowUp(value);
  }

  return (
    <div className="ask-body">
      <ConversationThread prompt={prompt} answer={answer ?? ""} turns={transcript} />
      <form
        className="ask-compose"
        onSubmit={(event) => {
          event.preventDefault();
          submitDraft();
        }}
      >
        {slashMenuOpen && slashCommands.length > 0 ? (
          <div className="ask-slash-menu" role="listbox" aria-label="Palette commands">
            {slashCommands.map((action, index) => {
              const Icon = actionIcon(action.subcommand);
              const meta = actionDisplayMeta(action);
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
                    runSlashCommand(action, argument);
                  }}
                >
                  <Icon size={15} strokeWidth={1.8} aria-hidden="true" />
                  <span>
                    <strong>{meta.label}</strong>
                    <small>{action.description}</small>
                  </span>
                  <code>/{action.subcommand}</code>
                </Button>
              );
            })}
          </div>
        ) : null}
        <Button
          variant="plain"
          size="unstyled"
          className="ask-attach"
          type="button"
          disabled={pending}
          aria-label="Attach context"
          title="Attach context"
        >
          <Paperclip size={18} strokeWidth={1.75} />
        </Button>
        <div className="ask-compose-input">
          <Input
            unstyled
            value={draft}
            disabled={pending}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (!slashMenuOpen || slashCommands.length === 0) return;
              if (event.key === "ArrowDown") {
                event.preventDefault();
                setSelectedCommand((index) => Math.min(index + 1, slashCommands.length - 1));
              } else if (event.key === "ArrowUp") {
                event.preventDefault();
                setSelectedCommand((index) => Math.max(index - 1, 0));
              } else if (event.key === "Tab" || event.key === "Enter") {
                event.preventDefault();
                const argument = draft.slice(1).trim().split(/\s+/).slice(1).join(" ");
                runSlashCommand(slashCommands[clampedSelectedCommand], argument);
              } else if (event.key === "Escape") {
                event.preventDefault();
                setDraft("");
              }
            }}
            placeholder={pending ? "Waiting for response..." : "Ask a follow-up..."}
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
