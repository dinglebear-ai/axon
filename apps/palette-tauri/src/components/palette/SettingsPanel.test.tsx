// @vitest-environment jsdom
//
// T-H3: behavioral render tests for SettingsPanel. The previous version only
// asserted `typeof`/`.name` and never mounted the component — it passed while
// the panel was fully broken. These tests render the real component and drive
// it with userEvent: type a server URL → assert onChange; click Save → assert
// onSave; toggle a switch → assert onChange. jest-dom matchers, jest-axe, and
// DOM polyfills are registered globally via src/test/setup.ts (Lane B).

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Mock the OAuth client so the AuthBlock's effect resolves deterministically and
// never reaches the real invoke seam during render tests.
const oauthState: { value: OauthStatus } = {
  value: { signedIn: false, scope: null, expiresAtUnix: null, serverUrl: null },
};

vi.mock("@/lib/oauthClient", async () => {
  const actual = await vi.importActual<typeof import("@/lib/oauthClient")>("@/lib/oauthClient");
  return {
    ...actual,
    oauthStatus: vi.fn(() => Promise.resolve(oauthState.value)),
    oauthLogin: vi.fn(() => Promise.resolve(oauthState.value)),
    oauthLogout: vi.fn(() => Promise.resolve(oauthState.value)),
  };
});

import type { PaletteConfig } from "@/lib/axonClient";
import type { OauthStatus } from "@/lib/oauthClient";
import { connectionFeedback, SettingsPanel } from "./SettingsPanel";

const baseConfig: PaletteConfig = {
  serverUrl: "http://127.0.0.1:8001",
  token: null,
  shortcut: "Ctrl+Shift+Space",
  collection: "axon",
  resultLimit: 10,
  theme: "system",
  hideOnBlur: true,
  openResultsInline: true,
  agentBubbles: false,
  showFooterHints: false,
};

function renderPanel(overrides: Partial<React.ComponentProps<typeof SettingsPanel>> = {}) {
  const onChange = vi.fn();
  const onClose = vi.fn();
  const onSave = vi.fn();
  render(
    <SettingsPanel
      configError={null}
      draftConfig={baseConfig}
      shortcutOptions={["Ctrl+Shift+Space", "Alt+Space", "Ctrl+Space"]}
      onChange={onChange}
      onClose={onClose}
      onSave={onSave}
      {...overrides}
    />,
  );
  return { onChange, onClose, onSave };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("SettingsPanel", () => {
  it("saves, removes, and deletes profile credentials without leaving the active handle", async () => {
    const user = userEvent.setup();
    const invoke = vi.spyOn(await import("@/lib/invoke"), "invoke").mockResolvedValue(undefined);
    const profile = { id: "labby-prod", label: "Labby", product: "labby" as const, origin: "https://labby.test", credentialHandle: null, pinnedServerId: "server-1", acceptedApiMajor: 1 as const };
    const { onChange } = renderPanel({ draftConfig: { ...baseConfig, backendProfiles: [profile], activeBackendProfiles: { labby: profile.id } } });
    await user.type(screen.getAllByPlaceholderText("unset - secret")[1], "secret-one");
    await user.click(screen.getByRole("button", { name: "Save credential" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_backend_credential", expect.objectContaining({ credential: expect.objectContaining({ profileId: profile.id, token: "secret-one" }) })));
    const saved = onChange.mock.calls.at(-1)?.[0] as PaletteConfig;
    expect(saved.backendProfiles?.[0].credentialHandle).toBe("labby:labby-prod");

    cleanup();
    renderPanel({ draftConfig: { ...saved, activeBackendProfiles: { labby: profile.id } }, onChange });
    await user.click(screen.getByRole("button", { name: "Remove credential" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("delete_backend_credential", { profileId: profile.id }));
    await user.click(screen.getByRole("button", { name: "Delete profile" }));
    const deleted = onChange.mock.calls.at(-1)?.[0] as PaletteConfig;
    expect(deleted.backendProfiles).toEqual([]);
    expect(deleted.activeBackendProfiles?.labby).toBeUndefined();
  });

  it("renders the connection settings with the server URL field", () => {
    renderPanel();
    expect(screen.getByText("Server URL")).toBeInTheDocument();
    expect(screen.getByDisplayValue("http://127.0.0.1:8001")).toBeInTheDocument();
  });

  it("hides desktop-only client controls on mobile", () => {
    renderPanel({ mobile: true });
    expect(screen.queryByText("Global shortcut")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Hide on blur/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Back" })).toBeInTheDocument();
    expect(screen.getByText("Max results")).toBeInTheDocument();
    expect(screen.getByText("Open results inline")).toBeInTheDocument();
  });

  it("lets mobile users leave settings from the top app bar", async () => {
    const user = userEvent.setup();
    const { onClose } = renderPanel({ mobile: true });
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("calls onChange when the server URL is edited", async () => {
    const user = userEvent.setup();
    const { onChange } = renderPanel();
    const input = screen.getByDisplayValue("http://127.0.0.1:8001");
    await user.type(input, "X");
    expect(onChange).toHaveBeenCalled();
    const last = onChange.mock.calls.at(-1)?.[0] as PaletteConfig;
    expect(last.serverUrl).toBe("http://127.0.0.1:8001X");
  });

  it("calls onSave when the Save button is clicked", async () => {
    const user = userEvent.setup();
    const { onSave } = renderPanel();
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it("calls onClose when the Close button is clicked", async () => {
    const user = userEvent.setup();
    const { onClose } = renderPanel();
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("calls onChange when the 'Hide on blur' switch is toggled", async () => {
    const user = userEvent.setup();
    const { onChange } = renderPanel();
    // "Hide on blur" is the first pressed MiniToggle in the connection tab.
    const toggles = screen.getAllByRole("button", { pressed: true });
    await user.click(toggles[0]);
    expect(onChange).toHaveBeenCalled();
    const last = onChange.mock.calls.at(-1)?.[0] as PaletteConfig;
    expect(last.hideOnBlur).toBe(false);
  });

  it("hides footer hints by default and lets the user enable them", async () => {
    const user = userEvent.setup();
    const { onChange } = renderPanel();

    const toggle = screen.getByRole("button", { name: /Show footer hints/i });
    expect(toggle).toHaveAttribute("aria-pressed", "false");

    await user.click(toggle);

    const last = onChange.mock.calls.at(-1)?.[0] as PaletteConfig;
    expect(last.showFooterHints).toBe(true);
  });

  it("only exposes Axon connection and palette-local settings", () => {
    renderPanel();
    expect(screen.getByText("Server URL")).toBeInTheDocument();
    expect(screen.getByText("Bearer token")).toBeInTheDocument();
    expect(screen.getByText("Global shortcut")).toBeInTheDocument();
    expect(screen.queryByText("Environment")).not.toBeInTheDocument();
    expect(screen.queryByText("config.toml")).not.toBeInTheDocument();
    expect(screen.queryByText("Collection")).not.toBeInTheDocument();
  });

  it("describes persisted connection test feedback", () => {
    expect(
      connectionFeedback({ status: "connected", checkedAt: 1, detail: "Doctor checks passed" }),
    ).toEqual({
      tone: "success",
      label: "Connected",
      detail: "Doctor checks passed",
    });
    expect(connectionFeedback({ status: "error", checkedAt: 1, detail: "HTTP 401" })).toEqual({
      tone: "error",
      label: "Connection failed",
      detail: "HTTP 401",
    });
  });
});

const authConfig: PaletteConfig = {
  serverUrl: "https://axon.example.com",
  token: null,
  shortcut: "Ctrl+Shift+Space",
  collection: "axon",
  resultLimit: 10,
  theme: "dark",
  hideOnBlur: false,
  openResultsInline: true,
  agentBubbles: false,
  showFooterHints: false,
};

describe("SettingsPanel authentication block", () => {
  beforeEach(() => {
    oauthState.value = { signedIn: false, scope: null, expiresAtUnix: null, serverUrl: null };
  });

  it("shows a Sign in button when signed out", async () => {
    render(
      <SettingsPanel
        configError={null}
        draftConfig={authConfig}
        shortcutOptions={["Ctrl+Shift+Space"]}
        onChange={() => {}}
        onClose={() => {}}
        onSave={() => {}}
      />,
    );
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /sign in with google/i })).toBeInTheDocument(),
    );
  });

  it("shows a Sign out button when signed in", async () => {
    // Far-future expiry → describeOauthStatus tone "success" → "Sign out" shown.
    oauthState.value = {
      signedIn: true,
      scope: "axon:read axon:write",
      expiresAtUnix: 4102444800,
      serverUrl: "https://axon.example.com",
    };
    render(
      <SettingsPanel
        configError={null}
        draftConfig={authConfig}
        shortcutOptions={["Ctrl+Shift+Space"]}
        onChange={() => {}}
        onClose={() => {}}
        onSave={() => {}}
      />,
    );
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /sign out/i })).toBeInTheDocument(),
    );
  });
});
