import { Component, lazy, Suspense, type ReactNode } from "react";

// Lazy boundary for the markdown renderer (P-H1). streamdown + the shiki code
// highlighter are the heaviest JS on the startup path, yet a fresh palette launch
// shows only the command bar + action list — no markdown. Splitting the renderer
// into its own chunk (MarkdownBodyInner) and loading it on first use moves that
// cost off the time-to-interactive path. The Suspense fallback is a plain <pre> so
// the raw text is still readable for the brief moment before the chunk resolves.
const MarkdownBodyInner = lazy(() => import("@/components/palette/MarkdownBodyInner"));

// A rejected `lazy()` import (chunk load failure: offline after a fresh deploy,
// asset-hash mismatch, blocked dynamic import) throws during render and is NOT
// caught by Suspense. Without a boundary it would unmount the whole React tree —
// a blank palette window with no message. This boundary degrades to the same raw
// <pre> the Suspense fallback already shows, so the user keeps their content.
class MarkdownErrorBoundary extends Component<{ raw: string; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  componentDidCatch(error: unknown) {
    console.error("[Axon Palette] markdown renderer failed to load", error);
  }
  render() {
    if (this.state.failed) {
      return <pre className="output-body output-code">{this.props.raw}</pre>;
    }
    return this.props.children;
  }
}

export function MarkdownBody({ children }: { children: string }) {
  return (
    <MarkdownErrorBoundary raw={children}>
      <Suspense fallback={<pre className="output-body output-code">{children}</pre>}>
        <MarkdownBodyInner>{children}</MarkdownBodyInner>
      </Suspense>
    </MarkdownErrorBoundary>
  );
}

export default MarkdownBody;
