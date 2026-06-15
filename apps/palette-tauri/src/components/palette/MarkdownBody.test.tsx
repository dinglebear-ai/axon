// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { MarkdownBody } from "./MarkdownBody";

// T-M3: lock in the hardened streamdown pipeline (S-M1/S-M2/S-L1). Crawled/RAG
// content flows through MarkdownBody, so a regression that loosened sanitize/harden
// would re-open XSS, tracking-beacon, and protocol-smuggling holes. MarkdownBody is
// lazy (P-H1), so every assertion waits for the chunk + render to settle.
async function renderMarkdown(markdown: string): Promise<HTMLElement> {
  const { container } = render(<MarkdownBody>{markdown}</MarkdownBody>);
  // The Suspense fallback is exactly `<pre class="output-body output-code">` and
  // Streamdown never emits that class combo, so wait until the fallback is gone —
  // i.e. the lazy chunk resolved and the real renderer mounted. Asserting on the
  // fallback (which is present synchronously) would test the wrong tree.
  await waitFor(() => {
    expect(container.querySelector("pre.output-body.output-code")).toBeNull();
  });
  return container;
}

describe("MarkdownBody sanitization", () => {
  it("strips raw <script> tags from rendered markdown", async () => {
    const container = await renderMarkdown("Hello\n\n<script>window.__pwned = true;</script>\n\nworld");
    expect(container.querySelector("script")).toBeNull();
    expect(container.textContent).toContain("Hello");
  });

  it("drops javascript: protocol links", async () => {
    const container = await renderMarkdown("[click me](javascript:alert(1))");
    // Streamdown renders links as a button (no raw <a href>), and no element may
    // carry a javascript: URL in href/src after sanitize + harden.
    expect(container.innerHTML).not.toMatch(/javascript:/i);
    expect(container.textContent).toContain("click me");
  });

  it("strips inline event handlers like onerror", async () => {
    const container = await renderMarkdown('<img src="x" onerror="window.__pwned = true">');
    // Whether the <img> survives or is dropped entirely, no element may keep the
    // event-handler attribute.
    expect(container.querySelector("[onerror]")).toBeNull();
  });

  it("removes remote images (allowedImagePrefixes: [])", async () => {
    const container = await renderMarkdown("![beacon](https://attacker.example/track.png?leak=1)");
    const remote = container.querySelector('img[src^="https://attacker.example"]');
    expect(remote).toBeNull();
  });

  it("rejects data: image payloads (allowDataImages: false)", async () => {
    const container = await renderMarkdown(
      "![x](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42m" + "NkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==)", // gitleaks:allow — 1x1 PNG test fixture, not a credential
    );
    expect(container.querySelector('img[src^="data:"]')).toBeNull();
  });

  it("keeps benign markdown content intact", async () => {
    const container = await renderMarkdown("# Title\n\nSome **bold** text and a [safe link](https://example.com).");
    expect(screen.getByRole("heading", { name: "Title" })).toBeInTheDocument();
    // Streamdown renders markdown links as a button element, not a raw anchor.
    const link = container.querySelector('[data-streamdown="link"]');
    expect(link).not.toBeNull();
    expect(link).toHaveTextContent("safe link");
    expect(container.textContent).toContain("bold");
  });
});
