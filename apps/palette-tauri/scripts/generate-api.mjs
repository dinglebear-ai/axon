import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const root = dirname(dirname(fileURLToPath(import.meta.url)));

// Default: use the repo-local OpenAPI spec so generation works offline and
// doesn't depend on a running production instance.
// Override with AXON_OPENAPI_URL=https://... to fetch from a live server,
// or pass --live as a CLI argument.
const useLive =
  process.argv.includes("--live") || Boolean(process.env.AXON_OPENAPI_URL);

let input;
if (useLive) {
  input =
    process.env.AXON_OPENAPI_URL || "https://axon.tootie.tv/api-docs/openapi.json";
  console.log(`[generate-api] fetching live spec from ${input}`);
} else {
  // Resolve relative to the monorepo root (two dirs up from apps/palette-tauri)
  const localSpec = resolve(root, "../../apps/web/openapi/axon.json");
  if (!existsSync(localSpec)) {
    console.error(
      `[generate-api] local spec not found at ${localSpec}\n` +
        `Run with --live or set AXON_OPENAPI_URL to fetch from a live server.`,
    );
    process.exit(1);
  }
  input = localSpec;
  console.log(`[generate-api] using local spec at ${input}`);
}

const bin =
  process.platform === "win32"
    ? join(root, "node_modules", ".bin", "openapi-typescript.cmd")
    : join(root, "node_modules", ".bin", "openapi-typescript");

const result = spawnSync(bin, [input, "-o", "src/lib/axon-api.d.ts"], {
  cwd: root,
  shell: process.platform === "win32",
  stdio: "inherit",
});

process.exit(result.status ?? 1);
