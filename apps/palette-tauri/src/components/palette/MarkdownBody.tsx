import { lazy, Suspense } from "react";

// Lazy boundary for the markdown renderer (P-H1). streamdown + the shiki code
// highlighter are the heaviest JS on the startup path, yet a fresh palette launch
// shows only the command bar + action list — no markdown. Splitting the renderer
// into its own chunk (MarkdownBodyInner) and loading it on first use moves that
// cost off the time-to-interactive path. The Suspense fallback is a plain <pre> so
// the raw text is still readable for the brief moment before the chunk resolves.
const MarkdownBodyInner = lazy(() => import("@/components/palette/MarkdownBodyInner"));

export function MarkdownBody({ children }: { children: string }) {
  return (
    <Suspense fallback={<pre className="output-body output-code">{children}</pre>}>
      <MarkdownBodyInner>{children}</MarkdownBodyInner>
    </Suspense>
  );
}

export default MarkdownBody;
