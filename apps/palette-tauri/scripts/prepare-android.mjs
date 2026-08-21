import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauriDir = join(appDir, "src-tauri");
const config = JSON.parse(await readFile(join(tauriDir, "tauri.conf.json"), "utf8"));

if (typeof config.identifier !== "string" || config.identifier.trim() === "") {
  throw new Error("tauri.conf.json is missing an Android package identifier");
}

const packageName = config.identifier.replaceAll("-", "_");
if (!/^[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)+$/.test(packageName)) {
  throw new Error(`invalid Android package name derived from Tauri identifier: ${packageName}`);
}

const templatePath = join(tauriDir, "android", "MainActivity.kt.template");
const template = await readFile(templatePath, "utf8");
if (!template.includes("__ANDROID_PACKAGE__")) {
  throw new Error(`Android MainActivity template is missing __ANDROID_PACKAGE__: ${templatePath}`);
}

const rendered = template.replaceAll("__ANDROID_PACKAGE__", packageName);
const destination = join(
  tauriDir,
  "gen",
  "android",
  "app",
  "src",
  "main",
  "java",
  ...packageName.split("."),
  "MainActivity.kt",
);

await mkdir(dirname(destination), { recursive: true });
let current = null;
try {
  current = await readFile(destination, "utf8");
} catch (error) {
  if (error?.code !== "ENOENT") throw error;
}

if (current !== rendered) {
  await writeFile(destination, rendered, "utf8");
  console.log(`Installed Axon Android MainActivity bridge: ${destination}`);
}
