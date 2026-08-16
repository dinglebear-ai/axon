const AxonApiError = (function () {
  function messageFromResponseText(text, fallback) {
    if (!text) return fallback;
    try {
      const payload = JSON.parse(text);
      if (typeof payload.message === "string" && payload.message) return payload.message;
      if (typeof payload.error === "string" && payload.error) return payload.error;
      if (payload.error && typeof payload.error.message === "string" && payload.error.message) {
        const code = typeof payload.error.code === "string" ? payload.error.code : "";
        return code ? `${payload.error.message} (${code})` : payload.error.message;
      }
    } catch {
      // A non-JSON error body is already the most useful server message.
    }
    return text || fallback;
  }

  return { messageFromResponseText };
})();

if (typeof module !== "undefined" && module.exports) {
  module.exports = { AxonApiError };
}
