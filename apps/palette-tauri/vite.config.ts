/// <reference types="vitest/config" />
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

// Dev-only: when running `vite dev` in a plain browser (no Tauri runtime), the
// palette's HTTP calls are same-origin `/v1/*` paths that this proxy forwards to
// a live `axon serve`. The bearer token is injected here so it never ships in the
// client bundle. Set AXON_DEV_SERVER + AXON_DEV_TOKEN when starting the dev server.
const devServer = process.env.AXON_DEV_SERVER ?? "http://127.0.0.1:8001";
const devToken = process.env.AXON_DEV_TOKEN ?? "";
const stripOrigin = process.env.AXON_DEV_STRIP_ORIGIN === "true";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    proxy: {
      "/v1": {
        target: devServer,
        changeOrigin: true,
        configure: (proxy) => {
          proxy.on("proxyReq", (proxyReq) => {
            // QA against the public reverse proxy may need an explicit
            // server-side-proxy mode: browser-origin POSTs can be rejected
            // before Axon handles auth. Keep this opt-in so normal dev still
            // exposes public-origin/CORS drift instead of masking it.
            if (stripOrigin) {
              proxyReq.removeHeader("origin");
              proxyReq.setHeader("x-axon-dev-proxy", "origin-stripped");
            }
            if (devToken) {
              proxyReq.setHeader("authorization", `Bearer ${devToken}`);
              proxyReq.setHeader("x-api-key", devToken);
            }
          });
        },
      },
    },
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  build: {
    // P-H1 / CI-M2: target the Tauri WebView floor. The desktop runtime is
    // WebKitGTK (Linux/macOS) and WebView2 (Windows), both well past ES2022 —
    // so we skip downleveling syntax that ships only to legacy browsers.
    // es2022 matches tsconfig's `target`.
    target: "es2022",
    rollupOptions: {
      output: {
        // P-H1 / CI-M2: split the heavy syntax-highlighting + markdown rendering
        // deps out of the main chunk so a cold palette launch (command bar +
        // action list, no markdown/code yet) does not pay their JS-init before
        // first interactive paint. Pairs with Lane R's React.lazy on the
        // markdown body — these chunks then load only when a result renders.
        manualChunks: {
          shiki: ["shiki"],
          streamdown: ["streamdown"],
        },
      },
    },
  },
  test: {
    // CI-M3: shared test infrastructure. setup.ts registers jest-dom +
    // jest-axe matchers and DOM polyfills that every lane's tests depend on.
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    css: false,
    coverage: {
      provider: "v8",
      reporter: ["text", "html"],
      // Coverage is gated on the lib layer only — the already-well-tested pure
      // helpers/hooks. Component coverage is intentionally not floored here
      // (jsdom render coverage is noisier and lands across other lanes). Keep
      // these realistic: a regression-guard floor, not a gold-plated target.
      include: ["src/lib/**/*.{ts,tsx}"],
      exclude: ["src/lib/**/*.test.{ts,tsx}", "src/lib/**/*.d.ts"],
      thresholds: {
        lines: 60,
        functions: 60,
        statements: 60,
        branches: 50,
      },
    },
  },
});
