import mermaid from "mermaid";
import type { DiagramPlugin } from "streamdown";

export const mermaidPlugin: DiagramPlugin = {
  name: "mermaid",
  type: "diagram",
  language: "mermaid",
  getMermaid(config) {
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme: "dark",
      fontFamily: "Manrope, Noto Sans, sans-serif",
      flowchart: { htmlLabels: false, curve: "basis" },
      ...config,
    });
    return mermaid;
  },
};
