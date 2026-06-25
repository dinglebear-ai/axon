import { OutputPanel } from "@/components/palette/OutputPanel";
import { ACTIONS } from "@/lib/actions";
import type { RunState } from "@/lib/runState";

const noop = () => {};
const askAction = ACTIONS.find((action) => action.subcommand === "ask");

const streamingRun: RunState = {
  kind: "streaming",
  title: "Streaming Ask question",
  subtitle: "POST /v1/ask/stream",
  text: "A skill is a reusable instruction pack",
  outputKind: "markdown",
  requestId: "fixture-stream",
  path: "/v1/ask/stream",
  actionLabel: "Ask question",
  prompt: "what is a skill?",
  transcript: [
    { id: "u1", role: "user", content: "what is a skill?" },
    {
      id: "a1",
      role: "assistant",
      content: "A skill is a reusable instruction pack",
      pending: true,
      activities: [
        { id: "act1", kind: "thinking", label: "Thinking", detail: "Planning retrieval and answer synthesis" },
        { id: "act2", kind: "tool", label: "Retrieving context", detail: "Querying collection axon" },
      ],
    },
  ],
};

const completeRun: RunState = {
  kind: "success",
  title: "Ask question",
  subtitle: "RAG over axon | /v1/ask/stream",
  text: "A skill is a reusable instruction pack that gives an agent domain-specific workflow, tools, and guardrails.",
  outputKind: "markdown",
  prompt: "what is a skill?",
  result: {
    ok: true,
    status: 0,
    method: "POST",
    path: "/v1/ask/stream",
    payload: { answer: "A skill is a reusable instruction pack that gives an agent domain-specific workflow, tools, and guardrails." },
  },
  transcript: [
    { id: "u1", role: "user", content: "what is a skill?" },
    {
      id: "a1",
      role: "assistant",
      content: "A skill is a reusable instruction pack that gives an agent domain-specific workflow, tools, and guardrails.",
      sources: [{ label: "docs.rs", url: "https://docs.rs" }],
    },
  ],
};

export function AskStreamTransitionFixture() {
  const state = new URLSearchParams(window.location.search).get("state");
  const run = state === "streaming" ? streamingRun : completeRun;
  if (!askAction) return null;
  return (
    <main className="fixture-shell fixture-shell-ask">
      <OutputPanel
        active={askAction}
        copied={false}
        outputKind="markdown"
        run={run}
        pinned={false}
        onCopy={noop}
        onRetry={noop}
        onFollowUp={noop}
        onHistory={noop}
        onCollapse={noop}
        onTogglePin={noop}
      />
    </main>
  );
}
