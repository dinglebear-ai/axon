// Shared helper for the extension's per-origin optional host permission
// (chrome-extension-contract.md "Permission Contract": "request host
// permissions only when needed" / "prefer `activeTab` for user-triggered
// capture"). manifest.json declares the Axon server origins as
// `optional_host_permissions` instead of a blanket always-on
// `host_permissions` grant; the extension requests the specific origin the
// configured Axon server lives at, gated on a user gesture (Options
// Save/Check API click handlers), and every fetch path checks the grant
// first so a missing permission fails with a clear message instead of a
// silent network error.
const AxonHostPermissions = (function () {
  function originPattern(serverUrl) {
    try {
      const parsed = new URL(serverUrl);
      return `${parsed.protocol}//${parsed.host}/*`;
    } catch {
      return null;
    }
  }

  async function hasAxonHostPermission(serverUrl) {
    const pattern = originPattern(serverUrl);
    if (!pattern) return false;
    if (!globalThis.chrome?.permissions?.contains) return true;
    try {
      return await chrome.permissions.contains({ origins: [pattern] });
    } catch {
      return false;
    }
  }

  // Must be called synchronously within a user gesture handler (click/keydown)
  // — Chrome silently resolves to `false` without prompting otherwise.
  async function requestAxonHostPermission(serverUrl) {
    const pattern = originPattern(serverUrl);
    if (!pattern) return false;
    if (!globalThis.chrome?.permissions?.request) return true;
    try {
      return await chrome.permissions.request({ origins: [pattern] });
    } catch {
      return false;
    }
  }

  return { originPattern, hasAxonHostPermission, requestAxonHostPermission };
})();

if (typeof module !== "undefined" && module.exports) {
  module.exports = { AxonHostPermissions };
}
