// @vitest-environment jsdom

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { SecretInput, SelectInput } from "./SettingsFields";

describe("SettingsFields", () => {
  it("keeps unset-option semantics while using the Aurora native select", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<SelectInput value="" options={["", "dark", "light"]} onChange={onChange} />);

    const select = screen.getByRole("combobox");
    expect(select).toHaveDisplayValue("(unset)");

    await user.selectOptions(select, "dark");
    expect(onChange).toHaveBeenCalledWith("dark");
  });

  it("keeps secret values hidden and suppressed from browser helpers until explicit reveal", async () => {
    const user = userEvent.setup();
    render(<SecretInput value="token-123" onChange={vi.fn()} />);

    const input = screen.getByDisplayValue("token-123");
    expect(input).toHaveAttribute("type", "password");
    expect(input).toHaveAttribute("autocomplete", "off");
    expect(input).toHaveAttribute("autocorrect", "off");
    expect(input).toHaveAttribute("autocapitalize", "off");
    expect(input).toHaveAttribute("spellcheck", "false");
    expect(input).toHaveAttribute("data-1p-ignore", "true");

    await user.click(screen.getByRole("button", { name: "Reveal secret" }));
    expect(input).toHaveAttribute("type", "text");
    expect(screen.getByRole("button", { name: "Hide secret" })).toBeInTheDocument();
  });
});
