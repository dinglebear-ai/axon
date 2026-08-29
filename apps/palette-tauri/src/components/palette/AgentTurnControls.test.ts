import { describe, expect, it } from "vitest";
import { validateApprovalChallenge } from "./AgentTurnControls";

const challenge = { executionContextId: "ctx-a", toolCallId: "call-a", expiresAtUnixMs: 2_000 };

describe("validateApprovalChallenge", () => {
  it("accepts only the current unexpired context and proposal", () => {
    expect(() => validateApprovalChallenge(challenge, "ctx-a", "call-a", 1_999)).not.toThrow();
    expect(() => validateApprovalChallenge(challenge, "ctx-b", "call-a", 1_000)).toThrow(
      "different execution context",
    );
    expect(() => validateApprovalChallenge(challenge, "ctx-a", "call-b", 1_000)).toThrow(
      "different execution context",
    );
    expect(() => validateApprovalChallenge(challenge, "ctx-a", "call-a", 2_000)).toThrow("expired");
  });
});
