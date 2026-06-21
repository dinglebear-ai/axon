// OAuth login client. Wraps the Rust Tauri commands through the shared invoke
// seam so the browser-dev path keeps working (never import @tauri-apps/* here).
import { invoke } from "./invoke";

export interface OauthStatus {
  signedIn: boolean;
  scope: string | null;
  expiresAtUnix: number | null;
  serverUrl: string | null;
}

/**
 * Read the current OAuth sign-in status from the Rust shell.
 *
 * Invokes the `axon_oauth_status` Tauri command, which inspects the saved
 * credential for the active server without triggering any browser flow.
 *
 * @returns The current {@link OauthStatus} (sign-in state, scope, expiry, and
 *   the server URL the credential belongs to).
 */
export function oauthStatus(): Promise<OauthStatus> {
  return invoke<OauthStatus>("axon_oauth_status");
}

/**
 * Begin the interactive Google OAuth sign-in flow for the active server.
 *
 * Invokes the `axon_oauth_login` Tauri command, which opens the system browser
 * to complete authorization and persists the resulting credential. Only
 * available in the desktop (Tauri) runtime.
 *
 * @returns The {@link OauthStatus} after a successful sign-in.
 */
export function oauthLogin(): Promise<OauthStatus> {
  return invoke<OauthStatus>("axon_oauth_login");
}

/**
 * Sign out of the active server, clearing its saved OAuth credential.
 *
 * Invokes the `axon_oauth_logout` Tauri command.
 *
 * @returns The {@link OauthStatus} after sign-out (typically signed-out).
 */
export function oauthLogout(): Promise<OauthStatus> {
  return invoke<OauthStatus>("axon_oauth_logout");
}

type Tone = "neutral" | "success" | "error";

/**
 * Map a raw {@link OauthStatus} to user-facing label/detail/tone copy.
 *
 * Encodes the presentation rules: a signed-in-but-expired session and a
 * credential belonging to a different server both surface as `"error"`, a valid
 * session as `"success"`, and a clean signed-out state as `"neutral"`.
 *
 * @param status - The OAuth status to describe.
 * @param nowUnix - Current Unix time in seconds, used to detect an expired
 *   session. Defaults to `Math.floor(Date.now() / 1000)`; pass an explicit value
 *   in tests for determinism.
 * @returns An object with a short `label`, a longer `detail` sentence, and a
 *   `tone` (`"neutral" | "success" | "error"`) for styling.
 */
export function describeOauthStatus(
  status: OauthStatus,
  nowUnix: number = Math.floor(Date.now() / 1000),
): { label: string; detail: string; tone: Tone } {
  if (status.signedIn) {
    const host = hostOf(status.serverUrl);
    if (status.expiresAtUnix != null && status.expiresAtUnix <= nowUnix) {
      return { tone: "error", label: "Session expired", detail: `Your ${host} session expired — sign in again.` };
    }
    return {
      tone: "success",
      label: "Signed in",
      detail: `Authorized to ${host}${status.scope ? ` (${status.scope})` : ""}.`,
    };
  }
  // Not signed in. If a credential exists for another server, explain it.
  if (status.serverUrl) {
    return {
      tone: "error",
      label: "Different server",
      detail: `Signed in to ${hostOf(status.serverUrl)}, not the current server — sign in again.`,
    };
  }
  return {
    tone: "neutral",
    label: "Not signed in",
    detail: "Sign in with Google to authorize this server via OAuth.",
  };
}

/**
 * Extract a display host (e.g. `axon.example.com`) from a server URL.
 *
 * @param serverUrl - The server URL to parse, or `null` when none is known.
 * @returns The URL host when parseable, the literal `serverUrl` string when it
 *   cannot be parsed as a URL, or the fallback `"the server"` when `null`.
 */
function hostOf(serverUrl: string | null): string {
  if (!serverUrl) return "the server";
  try {
    return new URL(serverUrl).host;
  } catch {
    return serverUrl;
  }
}
