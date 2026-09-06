import { Fragment, memo, useEffect, useMemo, useRef } from "react";

import { Message, MessageContent } from "@/components/aurora/ai/message";
import { Response } from "@/components/aurora/ai/response";
import { AgentTurnControls } from "@/components/palette/AgentTurnControls";
import { ActivityTrail, SourceStrip } from "@/components/palette/AskConversationBits";
import { AxonMark } from "@/components/palette/AxonMark";
import {
  ChatMessageActions,
  ChatSuggestionPanel,
  type SuggestionState,
} from "@/components/palette/ChatMessageAffordances";
import { MarkdownBody } from "@/components/palette/MarkdownBody";
import type { Client, PaletteConfig } from "@/lib/axonClient";
import type { AskTurn } from "@/lib/runState";

export const ConversationThread = memo(function ConversationThread({
  prompt,
  answer,
  turns,
  waiting = "Waiting for response...",
  reader = false,
  suggestionsEnabled = false,
  suggestionsByTurn = {},
  onSuggestTurn,
  onEditTurn,
  onRegenerateTurn,
  agentBubbles = false,
  client = null,
  config = null,
}: {
  prompt?: string;
  answer: string;
  turns?: AskTurn[];
  waiting?: string;
  reader?: boolean;
  suggestionsEnabled?: boolean;
  suggestionsByTurn?: Record<string, SuggestionState>;
  onSuggestTurn?: (turn: AskTurn) => void;
  onEditTurn?: (turn: AskTurn) => void;
  onRegenerateTurn?: (turn: AskTurn) => void;
  agentBubbles?: boolean;
  client?: Client | null;
  config?: PaletteConfig | null;
  action?: "ask" | "chat";
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
  const signature = threadTurns
    .map((turn) => `${turn.id}:${turn.content.length}:${turn.pending ? "pending" : "done"}`)
    .join("|");

  // biome-ignore lint/correctness/useExhaustiveDependencies: the derived turn signature is the scroll trigger.
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

  function previousUserTurn(turnIndex: number): AskTurn | undefined {
    for (let index = turnIndex - 1; index >= 0; index -= 1) {
      if (threadTurns[index]?.role === "user") return threadTurns[index];
    }
    return undefined;
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
          {answer ? (
            <MarkdownBody>{answer}</MarkdownBody>
          ) : (
            <span className="ask-waiting">{waiting}</span>
          )}
        </div>
      </div>
    );
  }

  return (
    // biome-ignore lint/a11y/useSemanticElements: named scroll region used by tests and assistive tech to identify the conversation transcript.
    <div
      ref={threadRef}
      className="ask-thread aurora-scrollbar"
      role="group"
      aria-label="Ask conversation"
      onScroll={onThreadScroll}
    >
      {threadTurns.map((turn, turnIndex) =>
        turn.role === "user" ? (
          <Fragment key={turn.id}>
            <Message
              className="ask-message ask-message-user"
              data-role="user"
              time="now"
              actions={
                <ChatMessageActions
                  enabled={suggestionsEnabled}
                  turn={turn}
                  suggestion={suggestionsByTurn[turn.id]}
                  onSuggest={onSuggestTurn}
                  onEdit={onEditTurn}
                  onRegenerate={onRegenerateTurn}
                />
              }
            >
              <MessageContent tone="user">
                <p>{turn.content}</p>
              </MessageContent>
            </Message>
            <ChatSuggestionPanel align="end" suggestion={suggestionsByTurn[turn.id]} />
          </Fragment>
        ) : (
          <Fragment key={turn.id}>
            {(() => {
              const regenerateSource = previousUserTurn(turnIndex);
              return (
                <Message
                  className="ask-message ask-message-assistant"
                  data-role="assistant"
                  time="now"
                  actions={
                    <ChatMessageActions
                      enabled={suggestionsEnabled}
                      turn={turn}
                      suggestion={suggestionsByTurn[turn.id]}
                      onSuggest={onSuggestTurn}
                      onEdit={onEditTurn}
                      onRegenerate={
                        regenerateSource ? () => onRegenerateTurn?.(regenerateSource) : undefined
                      }
                    />
                  }
                >
                  <span className="ask-assistant-avatar" role="img" aria-label="Axon" title="Axon">
                    <AxonMark size={18} />
                  </span>
                  <div className="ask-assistant-stack">
                    <ActivityTrail activities={turn.activities} pending={turn.pending} />
                    <MessageContent
                      tone="assistant"
                      streaming={Boolean(turn.pending)}
                      className={agentBubbles ? undefined : "aurora-message-content-plain"}
                    >
                      {turn.content ? (
                        <Response markdown={turn.content} streaming={Boolean(turn.pending)} />
                      ) : (
                        <span className="ask-waiting">{waiting}</span>
                      )}
                    </MessageContent>
                    <SourceStrip sources={turn.sources} />
                    {turn.agent ? (
                      <AgentTurnControls
                        agent={turn.agent}
                        loadout={turn.loadout}
                        client={client}
                        config={config}
                      />
                    ) : null}
                  </div>
                </Message>
              );
            })()}
            <ChatSuggestionPanel align="start" suggestion={suggestionsByTurn[turn.id]} />
          </Fragment>
        ),
      )}
    </div>
  );
});
