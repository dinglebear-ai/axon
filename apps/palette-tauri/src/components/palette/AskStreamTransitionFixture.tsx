import { OutputPanel } from "@/components/palette/OutputPanel";
import { ACTIONS } from "@/lib/actions";
import type { ChatSuggestion, RunState } from "@/lib/runState";

const noop = () => {};
const askAction = ACTIONS.find((action) => action.subcommand === "ask");
const chatAction = ACTIONS.find((action) => action.subcommand === "chat");

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

const chatRun: RunState = {
  kind: "success",
  title: "Chat with LLM",
  subtitle: "/v1/chat",
  text: "Claude Code plugins are packaged workflows with a manifest, optional skills, and distribution metadata.",
  outputKind: "markdown",
  prompt: "how do I create a Claude Code plugin?",
  result: {
    ok: true,
    status: 200,
    method: "POST",
    path: "/v1/chat",
    payload: { answer: "Claude Code plugins are packaged workflows with a manifest, optional skills, and distribution metadata." },
  },
  transcript: [
    { id: "u1", role: "user", content: "how do I create a Claude Code plugin?" },
    {
      id: "a1",
      role: "assistant",
      content: "Claude Code plugins are packaged workflows with a manifest, optional skills, and distribution metadata.",
    },
  ],
};

const fixtureSuggestions: ChatSuggestion[] = [
  {
    rank: 1,
    title: "Build plugins",
    url: "https://developers.openai.com/codex/build-plugins",
    snippet: "Create, test, and distribute plugins for Codex with a plugin manifest and marketplace entry.",
    score: 0.918,
  },
  {
    rank: 2,
    title: "Plugin creator skill",
    url: "https://developers.openai.com/codex/plugins",
    snippet: "Use @plugin-creator to scaffold plugin files and package metadata.",
    score: 0.874,
  },
];

export function AskStreamTransitionFixture() {
  const state = new URLSearchParams(window.location.search).get("state");
  const chatMode = state === "chat";
  const run = chatMode ? chatRun : state === "streaming" ? streamingRun : completeRun;
  const active = chatMode ? chatAction : askAction;
  if (!active) return null;
  return (
    <main className="fixture-shell fixture-shell-ask">
      <OutputPanel
        active={active}
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
        onRunAction={noop}
        onSuggestMessage={async () => fixtureSuggestions}
      />
    </main>
  );
}
