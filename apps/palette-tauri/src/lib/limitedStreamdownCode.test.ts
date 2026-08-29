import { describe, expect, it } from "vitest";

import { highlightCodeHtml, limitedCode } from "@/lib/limitedStreamdownCode";

describe("limitedStreamdownCode", () => {
  it("colors HTML file previews instead of falling back to monochrome text", async () => {
    const html = await highlightCodeHtml(
      '<section class="hero"><h1>Axon</h1></section>',
      "html",
    );

    expect(html).toContain('class="line"');
    expect(html).toMatch(/color:\s*#[0-9A-Fa-f]{6}/);
    expect(html).toContain("&#x3C;");
    expect(html).not.toContain('<section class="hero">');
  });

  it("shares common web-language aliases with chat code blocks", () => {
    expect(limitedCode.supportsLanguage("js")).toBe(true);
    expect(limitedCode.supportsLanguage("tsx")).toBe(true);
    expect(limitedCode.supportsLanguage("html")).toBe(true);
    expect(limitedCode.supportsLanguage("css")).toBe(true);
    expect(limitedCode.supportsLanguage("sql")).toBe(true);
  });
});
