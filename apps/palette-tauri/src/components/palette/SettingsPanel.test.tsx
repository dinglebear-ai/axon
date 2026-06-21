// @vitest-environment jsdom
//
// T-H3: behavioral render tests for SettingsPanel. The previous version only
// asserted `typeof`/`.name` and never mounted the component — it passed while
// the panel was fully broken. These tests render the real component and drive
// it with userEvent: type a server URL → assert onChange; click Save → assert
// onSave; toggle a switch → assert onChange. jest-dom matchers, jest-axe, and
// DOM polyfills are registered globally via src/test/setup.ts (Lane B).

import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
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

import { connectionFeedback, SettingsPanel } from "./SettingsPanel";
import type { PaletteConfig } from "@/lib/axonClient";
import type { OauthStatus } from "@/lib/oauthClient";

const baseConfig: PaletteConfig = {
  serverUrl: "http://127.0.0.1:8001",
  token: null,
  shortcut: "Ctrl+Shift+Space",
  collection: "axon",
  resultLimit: 10,
  theme: "system",
  hideOnBlur: true,
  openResultsInline: true,
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
  it("renders the connection tab with the server field", () => {
    renderPanel();
    expect(screen.getByText("Server")).toBeInTheDocument();
    expect(screen.getByDisplayValue("http://127.0.0.1:8001")).toBeInTheDocument();
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

  describe("tabs (A11Y-H2)", () => {
    it("exposes a tablist with three tabs and one selected tabpanel", () => {
      renderPanel();
      const tablist = screen.getByRole("tablist", { name: "Settings sections" });
      const tabs = within(tablist).getAllByRole("tab");
      expect(tabs).toHaveLength(3);

      const selected = within(tablist).getByRole("tab", { selected: true });
      expect(selected).toHaveTextContent("Connection");
      expect(selected).toHaveAttribute("aria-controls", "settings-tabpanel-connection");

      const panel = screen.getByRole("tabpanel");
      expect(panel).toHaveAttribute("aria-labelledby", "settings-tab-connection");
    });

    it("roves selection with the ArrowRight key", async () => {
      const user = userEvent.setup();
      renderPanel();
      const connectionTab = screen.getByRole("tab", { name: /Connection/ });
      connectionTab.focus();
      await user.keyboard("{ArrowRight}");
      const envTab = screen.getByRole("tab", { name: /Environment/ });
      expect(envTab).toHaveAttribute("aria-selected", "true");
      expect(envTab).toHaveFocus();
    });
  });

  it("describes persisted connection test feedback", () => {
    expect(connectionFeedback({ status: "connected", checkedAt: 1, detail: "Doctor checks passed" })).toEqual({
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
  envValues: {},
  configValues: {},
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
