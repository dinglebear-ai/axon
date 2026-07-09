import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { createEmptyConnectionDraft } from "@/lib/sftpModel";
import { SftpConnectionDialog } from "./SftpConnectionDialog";

describe("SftpConnectionDialog", () => {
  it("disables Connect until the draft is valid", () => {
    render(
      <SftpConnectionDialog
        draft={createEmptyConnectionDraft()}
        onChange={vi.fn()}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: /connect/i })).toBeDisabled();
  });

  it("enables Connect once host, username, and key path are filled", () => {
    const draft = { label: "prod", host: "example.com", port: 22, username: "deploy", privateKeyPath: "/k" };
    render(<SftpConnectionDialog draft={draft} onChange={vi.fn()} onSubmit={vi.fn()} onClose={vi.fn()} />);
    expect(screen.getByRole("button", { name: /connect/i })).not.toBeDisabled();
  });

  it("calls onSubmit with the draft when Connect is clicked", async () => {
    const onSubmit = vi.fn();
    const draft = { label: "prod", host: "example.com", port: 22, username: "deploy", privateKeyPath: "/k" };
    render(<SftpConnectionDialog draft={draft} onChange={vi.fn()} onSubmit={onSubmit} onClose={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: /connect/i }));
    expect(onSubmit).toHaveBeenCalledWith(draft);
  });
});
