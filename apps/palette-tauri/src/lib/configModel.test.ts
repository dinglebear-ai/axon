// Snapshot test (M2): ensures the configModel key lists do not silently drift
// from the .env.example and config.example.toml surfaces they mirror.
//
// When a new key is added to .env.example or config.example.toml, add it to
// the allowlist below AND to the corresponding ENV_GROUPS / CONFIG_GROUPS in
// configModel.ts.  The test will fail if the two lists diverge.

import { describe, expect, it } from "vitest";

import { CONFIG_GROUPS, ENV_GROUPS } from "./configModel";

// ── Expected env keys ──────────────────────────────────────────────────────
// Mirrors the keys the palette exposes in the Settings > Environment panel.
// Source of truth: ENV_GROUPS in configModel.ts, which itself mirrors
// .env.example.  When .env.example gains a new key, add it here too.
const EXPECTED_ENV_KEYS = [
  // Data & Service URLs
  "AXON_DATA_DIR",
  "AXON_HOME",
  "QDRANT_URL",
  "TEI_URL",
  "AXON_CHROME_REMOTE_URL",
  "AXON_COLLECTION",
  // Unified server auth
  "AXON_HTTP_TOKEN",
  "AXON_AUTH_MODE",
  "AXON_PUBLIC_URL",
  "AXON_GOOGLE_CLIENT_ID",
  "AXON_GOOGLE_CLIENT_SECRET",
  "AXON_AUTH_ADMIN_EMAIL",
  "AXON_ALLOWED_REDIRECT_URIS",
  "AXON_ALLOWED_ORIGINS",
  // Source Credentials
  "TAVILY_API_KEY",
  "AXON_SEARXNG_URL",
  "GITHUB_TOKEN",
  "GITLAB_TOKEN",
  "GITEA_TOKEN",
  "REDDIT_CLIENT_ID",
  "REDDIT_CLIENT_SECRET",
  "HF_TOKEN",
  // LLM Runtime
  "AXON_LLM_BACKEND",
  "AXON_OPENAI_BASE_URL",
  "AXON_SYNTHESIS_OPENAI_MODEL",
  "AXON_CHAT_OPENAI_MODEL",
  "AXON_OPENAI_API_KEY",
  "GEMINI_API_KEY",
  "GEMINI_HOME",
  "AXON_HEADLESS_GEMINI_HOME",
  "AXON_HEADLESS_GEMINI_CMD",
  "AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL",
  "AXON_CHAT_HEADLESS_GEMINI_MODEL",
  // HTTP Behavior
  "AXON_USER_AGENT",
  "AXON_CHROME_USER_AGENT",
  // Logging
  "AXON_LOG_PATH",
  "AXON_LOG_MAX_BYTES",
  // Docker / Compose
  "AXON_IMAGE",
  "AXON_HTTP_PUBLISH",
  "TEI_EMBEDDING_MODEL",
  "TEI_HTTP_PORT",
  "TEI_SERVER_MAX_CLIENT_BATCH_SIZE",
  "NVIDIA_VISIBLE_DEVICES",
  "CUDA_VISIBLE_DEVICES",
] as const;

// ── Expected config section prefixes ──────────────────────────────────────
// Top-level TOML section ids as they appear in CONFIG_GROUPS.  Adding a new
// section to config.example.toml means adding a new group in configModel.ts
// and a new entry here.
const EXPECTED_CONFIG_SECTIONS = [
  "providers-vector",
  "retrieval",
  "ask",
  "ask-cache",
  "ask-adaptive",
  "providers-embedding",
  "pipeline",
  "providers-render",
  "providers-fetch",
  "crawl",
  "crawl-verticals",
  "crawl-antibot",
  "providers-vector-payload",
] as const;

describe("configModel key snapshot (M2)", () => {
  it("ENV_GROUPS contains exactly the expected env keys", () => {
    const actualKeys = ENV_GROUPS.flatMap((g) => g.vars.map((v) => v.key));
    const expectedSet = new Set<string>(EXPECTED_ENV_KEYS);
    const actualSet = new Set<string>(actualKeys);

    const missing = [...expectedSet].filter((k) => !actualSet.has(k));
    const extra = [...actualSet].filter((k) => !expectedSet.has(k));

    expect(
      missing,
      `keys in snapshot but missing from ENV_GROUPS: ${missing.join(", ")}`,
    ).toHaveLength(0);
    expect(extra, `keys in ENV_GROUPS but missing from snapshot: ${extra.join(", ")}`).toHaveLength(
      0,
    );
  });

  it("CONFIG_GROUPS contains exactly the expected sections", () => {
    const actualSections = CONFIG_GROUPS.map((g) => g.id);
    const expectedSet = new Set<string>(EXPECTED_CONFIG_SECTIONS);
    const actualSet = new Set<string>(actualSections);

    const missing = [...expectedSet].filter((k) => !actualSet.has(k));
    const extra = [...actualSet].filter((k) => !expectedSet.has(k));

    expect(
      missing,
      `sections in snapshot but missing from CONFIG_GROUPS: ${missing.join(", ")}`,
    ).toHaveLength(0);
    expect(
      extra,
      `sections in CONFIG_GROUPS but missing from snapshot: ${extra.join(", ")}`,
    ).toHaveLength(0);
  });
});
