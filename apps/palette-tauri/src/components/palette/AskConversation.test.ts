// @ts-expect-error Vitest runs this file in Node; the app tsconfig intentionally omits Node globals.
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("AskConversation styles", () => {
  it("does not render markdown paragraphs as nested message bubbles", () => {
    const styles = readFileSync(new URL("../../styles.css", import.meta.url), "utf8");

    expect(styles).toContain(".ask-message > p");
    expect(styles).not.toContain(".ask-message p {");
  });
});
