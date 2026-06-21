// @vitest-environment jsdom
//
// Behavioral render test for AuthNotice. The component listens for the Rust
// shell's `palette://oauth-changed` event (e.g. a reactive 401 cleared a dead
// session on the action path) and, when the re-checked status is signed-out,
// surfaces an app-wide dismissible banner. This test proves the listener is
// registered, that firing it with a signed-out status shows the notice, and
// that the dismiss button clears it. jest-dom matchers and DOM polyfills are
// registered globally via src/test/setup.ts.

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Capture the latest `palette://oauth-changed` callback the component registers,
// via vi.hoisted so the hoisted vi.mock factory can write to it.
const listenRef = vi.hoisted(() => ({
  callback: null as null | ((event: unknown) => void),
}));

// Mock the invoke seam: the component only consumes `appWindow`. `listen`
// records the callback and returns a thenable resolving to a no-op unlisten so
// the effect's cleanup stays callable.
vi.mock("@/lib/invoke", () => ({
  isTauriRuntime: false,
  invoke: vi.fn(() => Promise.resolve(undefined)),
  appWindow: {
    listen: (_event: string, cb: (event: unknown) => void) => {
      listenRef.callback = cb;
      return Promise.resolve(() => {});
    },
  },
}));

// Mock the OAuth client so status resolves deterministically; reading mutable
// state lets a mid-test flip be observed on the next oauth-changed fire.
const oauthState: { value: OauthStatus } = {
  value: { signedIn: true, scope: null, expiresAtUnix: null, serverUrl: null },
};

vi.mock("@/lib/oauthClient", async () => {
  const actual = await vi.importActual<typeof import("@/lib/oauthClient")>("@/lib/oauthClient");
  return {
    ...actual,
    oauthStatus: vi.fn(() => Promise.resolve(oauthState.value)),
  };
});

import { AuthNotice } from "./AuthNotice";
import { oauthStatus } from "@/lib/oauthClient";
import type { OauthStatus } from "@/lib/oauthClient";

beforeEach(() => {
  listenRef.callback = null;
  oauthState.value = { signedIn: true, scope: null, expiresAtUnix: null, serverUrl: null };
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("AuthNotice", () => {
  it("shows then dismisses the notice when oauth-changed reports signed-out", async () => {
    render(<AuthNotice />);

    // Nothing rendered until a signed-out oauth-changed fires.
    expect(screen.queryByText(/signed out of Axon/i)).not.toBeInTheDocument();
    expect(listenRef.callback).toBeTypeOf("function");

    // A reactive 401 cleared the session → status flips signed-out, event fires.
    oauthState.value = { signedIn: false, scope: null, expiresAtUnix: null, serverUrl: null };
    listenRef.callback?.({});

    await waitFor(() =>
      expect(screen.getByText(/signed out of Axon/i)).toBeInTheDocument(),
    );

    // Dismiss clears it.
    fireEvent.click(screen.getByRole("button", { name: /dismiss/i }));
    await waitFor(() =>
      expect(screen.queryByText(/signed out of Axon/i)).not.toBeInTheDocument(),
    );
  });

  it("stays silent when oauth-changed reports still signed-in", async () => {
    render(<AuthNotice />);

    // Status remains signed-in; firing the event must not surface the banner.
    listenRef.callback?.({});

    // Wait for the async status check to actually run, then assert the banner
    // stayed hidden (rather than flushing a fixed number of microtasks).
    await waitFor(() => expect(oauthStatus).toHaveBeenCalled());
    expect(screen.queryByText(/signed out of Axon/i)).not.toBeInTheDocument();
  });
});
