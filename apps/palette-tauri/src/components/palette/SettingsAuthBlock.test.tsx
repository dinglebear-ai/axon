// @vitest-environment jsdom
//
// Behavioral render test for SettingsAuthBlock. The component re-fetches OAuth
// status both on mount and whenever the Rust shell emits
// `palette://oauth-changed` (e.g. a reactive 401 refresh cleared a dead
// session). This test proves the listener is registered for that event and that
// firing it re-runs `load()` — flipping the UI from "Sign out" to "Sign in with
// Google" once the underlying status goes signed-out. jest-dom matchers and DOM
// polyfills are registered globally via src/test/setup.ts.

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Capture the latest `palette://oauth-changed` registration the hook makes —
// both the event name and the callback. Declared via vi.hoisted so the hoisted
// vi.mock factory can write to it while the test body reads it.
const listenRef = vi.hoisted(() => ({
  event: null as null | string,
  callback: null as null | ((event: unknown) => void),
}));

// Mock the invoke seam: the hook only consumes `appWindow`. `listen` records the
// event name + callback and returns a thenable resolving to a no-op unlisten so
// the effect's cleanup stays callable. `invoke` is stubbed defensively.
vi.mock("@/lib/invoke", () => ({
  isTauriRuntime: false,
  invoke: vi.fn(() => Promise.resolve(undefined)),
  appWindow: {
    listen: (event: string, cb: (event: unknown) => void) => {
      listenRef.event = event;
      listenRef.callback = cb;
      return Promise.resolve(() => {});
    },
  },
}));

// Mock the OAuth client so status resolves deterministically. Keep the real
// describeOauthStatus (drives the tone → button selection); oauthStatus reads
// the current mutable state so a mid-test flip is observable on the next load().
const oauthState: { value: OauthStatus } = {
  value: {
    signedIn: true,
    scope: "axon:read axon:write",
    expiresAtUnix: 4102444800,
    serverUrl: "https://axon.example.com",
  },
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

import { SettingsAuthBlock } from "./SettingsAuthBlock";
import type { OauthStatus } from "@/lib/oauthClient";

beforeEach(() => {
  listenRef.event = null;
  listenRef.callback = null;
  oauthState.value = {
    signedIn: true,
    scope: "axon:read axon:write",
    expiresAtUnix: 4102444800,
    serverUrl: "https://axon.example.com",
  };
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("SettingsAuthBlock", () => {
  it("re-fetches status when palette://oauth-changed fires", async () => {
    render(<SettingsAuthBlock />);

    // Mounted signed-in → "Sign out" is shown.
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /sign out/i })).toBeInTheDocument(),
    );

    // The hook registered a listener for the oauth-changed event specifically.
    expect(listenRef.callback).toBeTypeOf("function");
    expect(listenRef.event).toBe("palette://oauth-changed");

    // Underlying status flips to signed-out; firing the captured callback must
    // re-run load() and flip the button to "Sign in with Google".
    oauthState.value = { signedIn: false, scope: null, expiresAtUnix: null, serverUrl: null };
    listenRef.callback?.({});

    await waitFor(() =>
      expect(screen.getByRole("button", { name: /sign in with google/i })).toBeInTheDocument(),
    );
  });
});
