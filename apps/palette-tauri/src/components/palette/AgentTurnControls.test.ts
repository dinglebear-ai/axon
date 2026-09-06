import { describe, expect, it } from "vitest";
import { validateApprovalChallenge } from "./AgentTurnControls";

const challenge = { approvalToken: "token-a", approvalId: "approval-a", expiresAtUnixMs: 2_000 };

describe("validateApprovalChallenge", () => {
  it("accepts only a complete unexpired wire-compatible challenge", () => {
    expect(() => validateApprovalChallenge(challenge, 1_999)).not.toThrow();
    expect(() => validateApprovalChallenge({ ...challenge, approvalId: "" }, 1_000)).toThrow("incomplete");
    expect(() => validateApprovalChallenge(challenge, 2_000)).toThrow("expired");
  });
});
