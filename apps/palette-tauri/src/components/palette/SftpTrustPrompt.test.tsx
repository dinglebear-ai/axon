import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { SftpTrustPrompt } from "./SftpTrustPrompt";

const entry = { host: "example.com", port: 22, keyType: "ssh-ed25519", fingerprint: "AA:BB:CC", firstSeenUnix: 0 };

describe("SftpTrustPrompt", () => {
  it("shows the host and fingerprint", () => {
    render(<SftpTrustPrompt entry={entry} onTrust={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByText(/example\.com/)).toBeInTheDocument();
    expect(screen.getByText(/AA:BB:CC/)).toBeInTheDocument();
  });

  it("calls onTrust when the user confirms", async () => {
    const onTrust = vi.fn();
    render(<SftpTrustPrompt entry={entry} onTrust={onTrust} onCancel={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: /trust/i }));
    expect(onTrust).toHaveBeenCalled();
  });

  it("calls onCancel without trusting when the user declines", async () => {
    const onTrust = vi.fn();
    const onCancel = vi.fn();
    render(<SftpTrustPrompt entry={entry} onTrust={onTrust} onCancel={onCancel} />);
    await userEvent.click(screen.getByRole("button", { name: /cancel/i }));
    expect(onCancel).toHaveBeenCalled();
    expect(onTrust).not.toHaveBeenCalled();
  });
});
