import { memo, useState } from "react";

import { MarkdownBody } from "@/components/palette/MarkdownBody";

// A read-only question→answer pair rendered with the ask bubble styling. Reused
// by the live ask view and the side-by-side evaluate view. Memoized so unrelated
// App re-renders during a stream don't recompute the thread (P-M1).
export const ConversationThread = memo(function ConversationThread({
  prompt,
  answer,
  assistantLabel = "Axon",
  waiting = "Waiting for response...",
  reader = false,
}: {
  prompt?: string;
  answer: string;
  assistantLabel?: string;
  waiting?: string;
  reader?: boolean;
}) {
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
    <div className="ask-thread aurora-scrollbar">
      {prompt ? (
        <div className="ask-message ask-message-user">
          <span>You</span>
          <p>{prompt}</p>
        </div>
      ) : null}
      <div className="ask-message ask-message-assistant">
        <span>{assistantLabel}</span>
        <div className="ask-answer">
          {answer ? <MarkdownBody>{answer}</MarkdownBody> : <span className="ask-waiting">{waiting}</span>}
        </div>
      </div>
    </div>
  );
});

// The full ask view: a conversation thread plus a follow-up compose box.
export const AskConversation = memo(function AskConversation({
  prompt,
  answer,
  pending,
  onFollowUp,
}: {
  prompt: string;
  answer: string;
  pending?: boolean;
  onFollowUp: (text: string) => void;
}) {
  const [draft, setDraft] = useState("");
  const canSend = draft.trim().length > 0 && !pending;
  return (
    <div className="ask-body">
      <ConversationThread prompt={prompt} answer={answer} reader={!pending} />
      <form
        className="ask-compose"
        onSubmit={(event) => {
          event.preventDefault();
          const value = draft.trim();
          if (!value || pending) return;
          setDraft("");
          onFollowUp(value);
        }}
      >
        <input
          value={draft}
          disabled={pending}
          onChange={(event) => setDraft(event.target.value)}
          placeholder={pending ? "Waiting for response..." : "Ask a follow-up..."}
          aria-label="Ask a follow-up"
        />
        <button type="submit" disabled={!canSend}>Send</button>
      </form>
    </div>
  );
});
