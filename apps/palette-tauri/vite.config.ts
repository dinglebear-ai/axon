import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

// Dev-only: when running `vite dev` in a plain browser (no Tauri runtime), the
// palette's HTTP calls are same-origin `/v1/*` paths that this proxy forwards to
// a live `axon serve`. The bearer token is injected here so it never ships in the
// client bundle. Set AXON_DEV_SERVER + AXON_DEV_TOKEN when starting the dev server.
const devServer = process.env.AXON_DEV_SERVER ?? "http://127.0.0.1:8001";
const devToken = process.env.AXON_DEV_TOKEN ?? "";

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
});
