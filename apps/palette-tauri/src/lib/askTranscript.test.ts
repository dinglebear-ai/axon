import { describe, expect, it } from "vitest";

import { answerParts, appendAskPendingTurn, completeLastAssistantTurn } from "./askTranscript";

describe("ask transcript helpers", () => {
  it("splits markdown source sections out of the answer body", () => {
    const parts = answerParts("Answer body.\n\n## Sources\n- [docs.rs](https://docs.rs)\n- https://example.com/page");

    expect(parts.answer).toBe("Answer body.");
    expect(parts.sources).toEqual([
      { label: "docs.rs", url: "https://docs.rs" },
      { label: "example.com", url: "https://example.com/page" },
    ]);
  });

  it("completes the pending assistant turn with extracted sources", () => {
    const pending = appendAskPendingTurn(undefined, "question", "r1");
    const complete = completeLastAssistantTurn(pending, "answer", [{ label: "docs.rs", url: "https://docs.rs" }]);

    expect(complete?.map((turn) => [turn.role, turn.content, turn.pending])).toEqual([
      ["user", "question", undefined],
      ["assistant", "answer", false],
    ]);
    expect(complete?.[1]?.sources?.[0]?.label).toBe("docs.rs");
  });
});
