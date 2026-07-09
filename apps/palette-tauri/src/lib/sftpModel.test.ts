import { describe, expect, it } from "vitest";

import { createEmptyConnectionDraft, isValidConnectionDraft, type SftpConnectionDraft } from "./sftpModel";

describe("createEmptyConnectionDraft", () => {
  it("returns an empty draft with port 22", () => {
    const draft = createEmptyConnectionDraft();
    expect(draft).toEqual({ label: "", host: "", port: 22, username: "", privateKeyPath: "" });
  });
});

describe("isValidConnectionDraft", () => {
  it("requires host, username, and privateKeyPath", () => {
    const draft: SftpConnectionDraft = {
      label: "prod",
      host: "",
      port: 22,
      username: "deploy",
      privateKeyPath: "/home/me/.ssh/id_ed25519",
    };
    expect(isValidConnectionDraft(draft)).toBe(false);
  });

  it("accepts a fully filled draft", () => {
    const draft: SftpConnectionDraft = {
      label: "prod",
      host: "example.com",
      port: 22,
      username: "deploy",
      privateKeyPath: "/home/me/.ssh/id_ed25519",
    };
    expect(isValidConnectionDraft(draft)).toBe(true);
  });

  it("rejects a port outside 1-65535", () => {
    const draft: SftpConnectionDraft = {
      label: "prod",
      host: "example.com",
      port: 0,
      username: "deploy",
      privateKeyPath: "/home/me/.ssh/id_ed25519",
    };
    expect(isValidConnectionDraft(draft)).toBe(false);
  });
});
