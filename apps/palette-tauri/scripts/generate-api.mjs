import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const input = process.env.AXON_OPENAPI_URL || "https://axon.tootie.tv/api-docs/openapi.json";
const bin = process.platform === "win32"
  ? join(root, "node_modules", ".bin", "openapi-typescript.cmd")
  : join(root, "node_modules", ".bin", "openapi-typescript");

const result = spawnSync(bin, [input, "-o", "src/lib/axon-api.d.ts"], {
  cwd: root,
  shell: process.platform === "win32",
  stdio: "inherit",
});

process.exit(result.status ?? 1);
