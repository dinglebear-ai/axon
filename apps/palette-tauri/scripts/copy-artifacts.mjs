import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(here, "..");
const repoRoot = path.resolve(appRoot, "../..");
const binDir = process.env.AXON_ARTIFACT_BIN_DIR || path.join(repoRoot, "bin");

const knownExecutables = [
  {
    source: path.join(appRoot, "src-tauri/target/release/axon-palette-tauri"),
    name: "axon-palette-release",
  },
  {
    source: path.join(appRoot, "src-tauri/target/release/axon-palette-tauri.exe"),
    name: "axon-palette-release.exe",
  },
  {
    source: path.join(
      appRoot,
      "src-tauri/target/x86_64-pc-windows-gnu/release/axon-palette-tauri.exe",
    ),
    name: "axon-palette-x86_64-pc-windows-gnu-release.exe",
  },
];

mkdirSync(binDir, { recursive: true });

let copied = 0;
for (const artifact of knownExecutables) {
  if (existsSync(artifact.source)) {
    copyFileSync(artifact.source, path.join(binDir, artifact.name));
    copied += 1;
  }
}

const bundleRoot = path.join(appRoot, "src-tauri/target/release/bundle");
if (existsSync(bundleRoot)) {
  for (const file of walk(bundleRoot)) {
    const ext = path.extname(file).toLowerCase();
    if (![".appimage", ".deb", ".dmg", ".msi", ".rpm", ".exe"].includes(ext)) continue;
    const relative = path.relative(bundleRoot, file).replaceAll(path.sep, "-");
    copyFileSync(file, path.join(binDir, `axon-palette-release-${relative}`));
    copied += 1;
  }
}

console.log(`Copied ${copied} palette artifact(s) to ${binDir}`);

function* walk(dir) {
  for (const entry of readdirSync(dir)) {
    const fullPath = path.join(dir, entry);
    const stat = statSync(fullPath);
    if (stat.isDirectory()) {
      yield* walk(fullPath);
    } else if (stat.isFile()) {
      yield fullPath;
    }
  }
}
