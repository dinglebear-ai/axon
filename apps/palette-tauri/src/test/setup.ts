// Vitest global setup (CI-M3 / shared test-infra contract).
//
// Wired via `test.setupFiles` in vite.config.ts so every test file in the
// palette gets these globals without re-importing them:
//   - @testing-library/jest-dom matchers (toBeInTheDocument, toHaveAttribute, …)
//   - jest-axe's `toHaveNoViolations` matcher (accessibility assertions)
//   - jsdom DOM polyfills that jsdom does not implement but the palette uses
//     (matchMedia for prefers-* queries, scrollIntoView for list focus,
//     ResizeObserver for layout-aware components).
//
// Other lanes' tests assume these are registered — do NOT re-stub them locally.

import "@testing-library/jest-dom/vitest";
import { expect } from "vitest";
import { toHaveNoViolations } from "jest-axe";

expect.extend(toHaveNoViolations);

// --- DOM polyfills not provided by jsdom -----------------------------------

if (typeof window !== "undefined" && typeof window.matchMedia !== "function") {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: (query: string): MediaQueryList =>
      ({
        matches: false,
        media: query,
        onchange: null,
        addListener: () => {},
        removeListener: () => {},
        addEventListener: () => {},
        removeEventListener: () => {},
        dispatchEvent: () => false,
      }) as unknown as MediaQueryList,
  });
}

if (
  typeof Element !== "undefined" &&
  typeof Element.prototype.scrollIntoView !== "function"
) {
  Element.prototype.scrollIntoView = () => {};
}

if (typeof globalThis.ResizeObserver === "undefined") {
  class ResizeObserverPolyfill {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  }
  globalThis.ResizeObserver =
    ResizeObserverPolyfill as unknown as typeof ResizeObserver;
}
