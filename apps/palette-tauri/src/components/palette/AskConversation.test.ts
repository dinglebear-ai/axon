// @ts-expect-error Vitest runs this file in Node; the app tsconfig intentionally omits Node globals.
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("AskConversation styles", () => {
  it("does not render markdown paragraphs as nested message bubbles", () => {
    // Read from the cwd-relative path (vitest runs with the package root as cwd),
    // matching OperationResultView.test.tsx. `new URL(..., import.meta.url)` builds
    // a non-file URL under this vitest config and throws "URL must be of scheme file".
    const styles = readFileSync("src/styles.css", "utf8");

    expect(styles).toContain(".aurora-message-content");
    expect(styles).not.toContain(".ask-message > p");
    expect(styles).not.toContain(".ask-message p {");
  });
});
