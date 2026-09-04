import { useEffect, useRef, useState } from "react";

export function HighlightedFileCode({
  code,
  language,
  searchQuery = "",
}: {
  code: string;
  language: string;
  searchQuery?: string;
}) {
  const [html, setHtml] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let active = true;
    setHtml(null);
    void import("@/lib/limitedStreamdownCode")
      .then(({ highlightCodeHtml }) => highlightCodeHtml(code, language))
      .then((value) => {
        if (active) setHtml(value);
      })
      .catch((error: unknown) => {
        console.error("[Axon Palette] Failed to highlight file preview", error);
      });
    return () => {
      active = false;
    };
  }, [code, language]);

  useEffect(() => {
    const root = rootRef.current;
    const query = searchQuery.trim();
    if (!root || !html) return;
    // Restore pristine Shiki markup before applying a new search so changing
    // the query never nests or leaves stale <mark> nodes behind.
    root.innerHTML = html;
    if (!query) return;
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    const textNodes: Text[] = [];
    while (walker.nextNode()) textNodes.push(walker.currentNode as Text);
    const needle = query.toLocaleLowerCase();
    for (const node of textNodes) {
      const value = node.nodeValue ?? "";
      const lower = value.toLocaleLowerCase();
      if (!lower.includes(needle)) continue;
      const fragment = document.createDocumentFragment();
      let cursor = 0;
      let index = lower.indexOf(needle);
      while (index >= 0) {
        fragment.append(value.slice(cursor, index));
        const mark = document.createElement("mark");
        mark.className = "files-search-match";
        mark.textContent = value.slice(index, index + query.length);
        fragment.append(mark);
        cursor = index + query.length;
        index = lower.indexOf(needle, cursor);
      }
      fragment.append(value.slice(cursor));
      node.replaceWith(fragment);
    }
  }, [html, searchQuery]);

  if (!html) return <code>{code}</code>;
  // biome-ignore lint/security/noDangerouslySetInnerHtml: Shiki escapes source text before returning trusted highlighted markup.
  return <div ref={rootRef} className="files-shiki" dangerouslySetInnerHTML={{ __html: html }} />;
}
