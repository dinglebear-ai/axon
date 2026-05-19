# Spider AI/Agent Capabilities: Alignment Report for axon_rust

**Generated:** 2026-02-19
**Agent:** ai-agent-investigator (Task #2)
**Scope:** spider_agent library + AI examples vs axon_rust extract/ask/vector ops

---

## Table of Contents

1. [spider_agent Architecture](#1-spider_agent-architecture)
2. [Multimodal/Vision Capabilities](#2-multimodalvision-capabilities)
3. [Concurrent AI Extraction Patterns](#3-concurrent-ai-extraction-patterns)
4. [Anti-Bot AI (not_a_robot examples)](#4-anti-bot-ai-not_a_robot-examples)
5. [LLM Provider Coverage](#5-llm-provider-coverage)
6. [axon_rust Current State](#6-axon_rust-current-state)
7. [Gap Analysis](#7-gap-analysis)
8. [Integration Proposal: cortex agent Command](#8-integration-proposal-cortex-agent-command)
9. [Priority Ranking](#9-priority-ranking)

---

## 1. spider_agent Architecture

### What It Is

`spider_agent` is a **concurrent-safe multimodal agent library** designed to be wrapped in `Arc` for multi-task access. It is a separate crate from spider core — a higher-level orchestration layer that glues together LLM providers, search providers, browser automation, and custom tools into a single composable `Agent` struct.

**Crate:** `spider_agent = "2.45.20"` at `~/workspace/spider/spider_agent/`

### Core Struct: `Agent`

```rust
pub struct Agent {
    llm: Option<Box<dyn LLMProvider>>,
    client: reqwest::Client,
    search_provider: Option<Box<dyn SearchProvider>>,  // feature-gated
    browser: Option<BrowserContext>,                    // feature-gated chrome
    webdriver: Option<WebDriverContext>,                // feature-gated webdriver
    temp_storage: Option<TempStorage>,                  // feature-gated fs
    memory: AgentMemory,                               // lock-free via DashMap
    llm_semaphore: Arc<Semaphore>,                     // concurrent call cap
    config: AgentConfig,
    usage: Arc<UsageStats>,                            // atomic counters
    custom_tools: CustomToolRegistry,
}
```

### Builder Pattern

```rust
let agent = Arc::new(
    Agent::builder()
        .with_openai("sk-...", "gpt-4o-mini")           // LLM provider
        .with_openai_compatible(api_url, key, model)     // any OpenAI-compat
        .with_search_serper("serper-key")                // search provider
        .with_spider_cloud("spider-cloud-api-key")       // custom tools
        .with_browser(browser_ctx)                       // Chrome automation
        .with_max_concurrent_llm_calls(10)               // semaphore limit
        .with_system_prompt("You are...")
        .build()?
);
```

### Key Methods

| Category | Method | Description |
|----------|--------|-------------|
| **Search** | `agent.search(query)` | Web search via configured provider |
| **Search** | `agent.search_with_options(query, opts)` | With country/lang/domain filters |
| **LLM** | `agent.prompt(messages)` | Raw LLM call → String |
| **LLM** | `agent.complete(messages)` | Full `CompletionResponse` with token usage |
| **Extract** | `agent.extract(html, prompt)` | HTML → JSON via LLM |
| **Extract** | `agent.extract_structured(html, schema)` | HTML + JSON schema → JSON |
| **HTTP** | `agent.fetch(url)` | Fetch URL → `FetchResult { html, status, content_type }` |
| **Research** | `agent.research(topic, opts)` | Search + extract + synthesize pipeline |
| **Memory** | `agent.memory_get/set/clear(key)` | Lock-free DashMap session store |
| **Usage** | `agent.usage()` | `UsageSnapshot` with token + call counts |
| **Browser** | `agent.navigate/click/type_text/screenshot()` | Chrome DOM automation |
| **Tools** | `agent.execute_custom_tool(name, path, query, body)` | Custom HTTP tool |
| **Tools** | `agent.register_spider_cloud(config)` | Register Spider Cloud routes |

### Module Structure

```
spider_agent/src/
├── lib.rs          — re-exports everything
├── agent.rs        — Agent struct + AgentBuilder
├── config.rs       — AgentConfig, UsageLimits, UsageStats, SearchOptions, ResearchOptions
├── error.rs        — AgentError, AgentResult, SearchError
├── llm/            — LLMProvider trait, OpenAIProvider, Message, CompletionResponse
├── memory.rs       — AgentMemory (DashMap-backed)
├── tools.rs        — CustomTool, CustomToolRegistry, SpiderCloudToolConfig
├── search/         — SearchProvider trait, SerperProvider, BraveProvider, BingProvider, TavilyProvider
├── browser.rs      — BrowserContext (chromey-backed)
├── webdriver.rs    — WebDriverContext (thirtyfour-backed)
├── temp.rs         — TempStorage (tempfile-backed)
└── automation/     — RemoteMultimodalEngine, concurrent chains, planning, schema gen
```

### Feature Flags (opt-in)

| Feature | Adds |
|---------|------|
| `openai` | OpenAI + OpenAI-compatible LLM provider via `async-openai` |
| `chrome` | Browser automation via `chromey` |
| `webdriver` | Browser automation via `thirtyfour` |
| `search_serper` | Serper.dev search |
| `search_brave` | Brave Search |
| `search_bing` | Bing Search |
| `search_tavily` | Tavily AI Search |
| `fs` | Temp filesystem storage |
| `skills` | Dynamic skill loading (`spider_skills`) for CAPTCHA bypass |
| `memvid` | Long-term experience memory via `memvid-rs` |
| `full` | All of the above |

### Key Automation Types (from `automation/` module)

These are re-exported directly from `spider` core's `features::automation`:

```rust
// Automation engine for browser-based LLM control
pub struct RemoteMultimodalEngine { ... }
pub struct RemoteMultimodalConfigs {
    pub api_key: Option<String>,
    pub system_prompt: Option<String>,
    pub user_message_extra: Option<String>,
    pub cfg: RemoteMultimodalConfig,
    // api_url, model fields
}

// Performance preset configs
RemoteMultimodalConfig::fast()              // tool calling + HTML diff + confidence + concurrent
RemoteMultimodalConfig::fast_with_planning() // fast + multi-step planning + self-healing

// Advanced feature structs
pub struct ToolCallingMode { Auto, JsonObject, Disabled }
pub struct HtmlDiffMode { Auto, Enabled, Disabled }  // 50-70% token reduction
pub struct PlanningModeConfig { ... }      // multi-step planning
pub struct SelfHealingConfig { ... }       // auto-repair failed CSS selectors
pub struct ConfidenceRetryStrategy { ... } // smarter retry decisions
pub struct DependencyGraph { ... }         // concurrent action chains
```

---

## 2. Multimodal/Vision Capabilities

### What spider Supports

Spider's `remote_multimodal` system (accessed via `Website::with_remote_multimodal(config)`) supports:

1. **Screenshot capture + vision inference**: Chrome takes screenshots, sends them to a vision-capable LLM (GPT-4o, Claude Opus, Qwen VL, Gemini) alongside page HTML
2. **Structured JSON extraction from visual + HTML content**: Configured via `mm_config.cfg.extra_ai_data = true` + `request_json_object = true`
3. **Multi-provider routing**: OpenRouter, OpenAI-direct, Gemini, any OpenAI-compatible endpoint
4. **Per-page result streaming**: Results arrive via `page.extra_remote_multimodal_data` in the subscribe channel

### Example: remote_multimodal.rs Pattern

```rust
// Spider side — configure extraction with vision model
let mut mm_config = RemoteMultimodalConfigs::new(
    "https://openrouter.ai/api/v1/chat/completions",
    "qwen/qwen-2-vl-72b-instruct",   // vision model
);
mm_config.api_key = Some(api_key);
mm_config.system_prompt = Some("Extract book details as JSON...".to_string());
mm_config.cfg.extra_ai_data = true;
mm_config.cfg.include_html = true;
mm_config.cfg.include_title = true;
mm_config.cfg.include_url = true;
mm_config.cfg.max_rounds = 1;
mm_config.cfg.request_json_object = true;

let mut website = Website::new(url)
    .with_limit(1)
    .with_remote_multimodal(Some(mm_config))
    .build()?;

// Results arrive as page.extra_remote_multimodal_data
let mut rx = website.subscribe(16)?;
while let Ok(page) = rx.recv().await {
    if let Some(ref ai_data) = page.extra_remote_multimodal_data {
        for result in ai_data.iter() {
            println!("Extracted: {}", result.content_output);
            println!("Usage: {:?}", result.usage);  // token tracking
        }
    }
}
```

### What axon_rust Is Missing

axon_rust's `remote_extract.rs` does NOT use `with_remote_multimodal`. Instead it:
1. Crawls pages via `Website::new().build()` + `subscribe(16)`
2. Gets raw HTML via `page.get_html()`
3. Manually sends HTML to OpenAI-compatible endpoint via `reqwest`

**Gaps vs spider multimodal:**
- No vision/screenshot integration (text-only extraction)
- No `extra_remote_multimodal_data` usage (bypasses spider's built-in AI pipeline)
- No `RemoteMultimodalConfig` features: tool calling, HTML diff, planning, self-healing
- No multi-model routing (one hardcoded OpenAI endpoint)
- No per-model token tracking via spider's `AutomationUsage`

---

## 3. Concurrent AI Extraction Patterns

### Spider's Pattern (concurrent_ai_extraction.rs)

Spider's concurrent extraction works at the **crawl level**: the `RemoteMultimodalConfigs` is passed into `Website::with_remote_multimodal(config)`, which means **every page visited by the crawler automatically gets AI-extracted** concurrently as the crawler visits pages. No manual spawning required.

```rust
// Configure once, apply to every crawled page automatically
let mut mm_config = RemoteMultimodalConfigs::new(&api_url, &model);
mm_config.cfg.extraction_prompt = Some("Extract book details...".to_string());

let mut website = Website::new("https://books.toscrape.com/catalogue/page-1.html")
    .with_limit(10)
    .with_remote_multimodal(Some(mm_config))
    .build()?;

// ALL 10 pages get AI-extracted concurrently during crawl
let mut rx = website.subscribe(16)?;
let handle = tokio::spawn(async move {
    let mut books = Vec::new();
    while let Ok(page) = rx.recv().await {
        if let Some(ref ai_data) = page.extra_remote_multimodal_data {
            // Results land here as pages complete
            for result in ai_data.iter() {
                if result.content_output.get("title").is_some() {
                    books.push(result.content_output.clone());
                }
            }
        }
    }
    books
});

website.crawl_raw().await;
website.unsubscribe();
let books = handle.await?;
```

### axon_rust's Pattern (remote_extract.rs)

axon_rust does concurrent extraction differently — it **serially** processes each page in the subscribe loop, sending HTTP requests to the LLM API one at a time per page:

```rust
// Current axon_rust approach — serial LLM calls per page
while let Ok(page) = rx.recv().await {
    let html = page.get_html();
    // Blocking serial LLM call per page
    if let Ok(items) = extract_items_fallback(&client, &api_url, &api_key, &model, ...).await {
        all_results.extend(items);
    }
}
```

**Problem**: The subscribe loop is a single tokio task. Each `extract_items_fallback` call is awaited sequentially. Pages coming in faster than the LLM can process them will queue up. This is not truly concurrent AI extraction.

### Spider's Agent-Level Concurrent Pattern (spider_agent)

The `spider_agent` `Agent` struct supports truly concurrent LLM calls via `Arc` + semaphore:

```rust
let agent = Arc::new(Agent::builder()
    .with_openai(key, "gpt-4o")
    .with_max_concurrent_llm_calls(10)  // Semaphore = 10 concurrent
    .build()?);

// True concurrent extraction across multiple pages
let mut handles = Vec::new();
for url in urls {
    let agent = agent.clone();  // Arc clone — zero cost
    handles.push(tokio::spawn(async move {
        let fetch = agent.fetch(&url).await?;
        agent.extract(&fetch.html, "Extract product data...").await
    }));
}
// All 10 run truly concurrently, limited by semaphore
let results: Vec<_> = futures::future::join_all(handles).await;
```

**Key difference**: axon_rust serially awaits each LLM call. Spider's multimodal engine runs extraction concurrently at the crawler level. The `spider_agent` approach allows explicit concurrency control via `Arc<Agent>`.

---

## 4. Anti-Bot AI (not_a_robot Examples)

### What Spider Demonstrates

The `not_a_robot` examples show spider's AI being used for **interactive browser challenge solving** — not just passive extraction. Key capabilities:

**not_a_robot.rs** (OpenRouter/Claude):
- Uses `RemoteMultimodalConfigs` with a vision-capable model (Claude Opus 4.6 default)
- Sends screenshots of the current page state to the LLM
- LLM returns structured actions: `Navigate`, `Click`, `Type`, `Scroll`, `Wait`
- Spider executes actions and feeds the result (new screenshot) back to the LLM
- Multi-round: up to `max_rounds` iterations per page
- `user_message_extra` provides task instructions; system prompt handles action parsing
- Memory ops (`memory_ops`) let the LLM track state across challenge levels

**not_a_robot_chrome_ai.rs** (Chrome built-in Gemini Nano):
- Uses Chrome's `LanguageModel` API (Prompt API) — no external API key
- Requires Chrome Canary with Optimization Guide On Device Model component
- Runs Gemini Nano locally in the browser process
- Same `RemoteMultimodalConfigs` interface, `use_chrome_ai: true` flag
- Hardware requirements: 22 GB storage, 4 GB VRAM or 16 GB RAM

**not_a_robot_haiku.rs** (Claude Haiku via OpenRouter):
- Lighter/faster variant using Claude Haiku for cheaper challenge solving

### What This Means for axon_rust

axon_rust has **zero** interactive automation capability. Its extract command is passive: fetch HTML → send to LLM → get JSON back. It cannot:
- Click buttons or navigate SPAs
- Handle CAPTCHAs or interactive challenges
- Maintain multi-round LLM dialogue about page state
- Use vision (screenshot) to understand page layout when HTML is unhelpful

The anti-bot AI capability requires the `spider/chrome` + `spider/agent_chrome` + `spider/agent_skills` features and the `RemoteMultimodalConfigs` engine — none of which axon_rust currently enables or uses.

---

## 5. LLM Provider Coverage

### Spider's LLM Support

| Provider | Spider Integration | How |
|----------|-------------------|-----|
| **OpenAI** | Native via `GPTConfigs` | `Website::with_openai(config)` |
| **Gemini** | Native via `GeminiConfigs` | `Website::with_gemini(config)` |
| **Any OpenAI-compatible** | Via `RemoteMultimodalConfigs` | `Website::with_remote_multimodal(config)` |
| **OpenRouter** | Via `RemoteMultimodalConfigs` (any model) | `api_url = "https://openrouter.ai/api/v1/chat/completions"` |
| **Chrome Gemini Nano** | Local on-device via `use_chrome_ai` flag | Chrome's LanguageModel API |
| **Anthropic (via OpenRouter)** | Via `RemoteMultimodalConfigs` | `model = "anthropic/claude-opus-4.6"` |

In spider_agent:
| Provider | Feature Flag | Builder Method |
|----------|-------------|----------------|
| OpenAI | `openai` | `.with_openai(key, model)` |
| Any OpenAI-compat | `openai` | `.with_openai_compatible(url, key, model)` |
| Gemini | Not directly — use OpenAI-compat via Google's API | `.with_openai_compatible(...)` |
| Serper search | `search_serper` | `.with_search_serper(key)` |
| Brave search | `search_brave` | `.with_search_brave(key)` |
| Bing search | `search_bing` | `.with_search_bing(key)` |
| Tavily search | `search_tavily` | `.with_search_tavily(key)` |

### axon_rust's LLM Support

Currently single-provider, hardcoded pattern:

```rust
// remote_extract.rs — single OpenAI-compatible endpoint, no abstraction
let api_url = format!("{}/chat/completions", openai_base_url.trim_end_matches('/'));
client.post(api_url)
    .bearer_auth(openai_api_key)
    .json(&serde_json::json!({
        "model": openai_model,
        "messages": [...],
        "response_format": {"type": "json_object"},
        "temperature": 0.1
    }))
```

**Gap**: axon_rust supports exactly one LLM provider pattern. Spider supports 6+ LLM providers + 4 search providers. The `spider_agent` `LLMProvider` trait would allow adding Anthropic-direct, Gemini-direct, or local Ollama providers cleanly.

---

## 6. axon_rust Current State

### What Exists

**extract command** (`crates/extract/remote_extract.rs` + `crates/cli/commands/extract.rs`):
- Spider subscribe + crawl loop
- Serial per-page HTTP POST to OpenAI-compatible endpoint
- `DeterministicExtractionEngine` with rule-based parsers (tries deterministic parse first, LLM fallback)
- Token counting, cost estimation, parser hit tracking
- AMQP job queue with status/cancel/list/cleanup subcommands

**ask command** (vector search → LLM synthesis using Qdrant + TEI):
- Semantic search in Qdrant
- Feeds chunks as context to OpenAI-compatible LLM
- Static single-endpoint pattern

### What Is Missing

1. `spider_agent` dependency not in `Cargo.toml` at all
2. No `RemoteMultimodalConfigs` usage anywhere
3. No vision/screenshot integration
4. No concurrent LLM calls (serial per-page)
5. No session memory (`AgentMemory`)
6. No multi-provider LLM abstraction (`LLMProvider` trait)
7. No search providers (Serper/Brave/Bing/Tavily)
8. No agentic loop (multi-round browser interaction)
9. No schema generation from examples
10. No HTML diff optimization (50-70% token reduction)
11. No planning mode, self-healing selectors, or confidence retry

---

## 7. Gap Analysis

### Side-by-Side Comparison

| Capability | axon_rust | spider/spider_agent |
|-----------|-----------|---------------------|
| Single-page extraction | YES (serial) | YES (concurrent) |
| Concurrent extraction | NO (serial loop) | YES (Arc<Agent> + semaphore) |
| Vision extraction | NO | YES (screenshots → LLM) |
| Multi-model routing | NO | YES (OpenAI/Gemini/OpenRouter/local) |
| Search integration | NO | YES (4 providers) |
| Interactive automation | NO | YES (click/type/navigate/scroll) |
| CAPTCHA/anti-bot | NO | YES (not_a_robot examples) |
| Multi-round LLM dialogue | NO | YES (max_rounds config) |
| Session memory | NO | YES (AgentMemory/DashMap) |
| HTML diff optimization | NO | YES (50-70% token reduction) |
| Planning mode | NO | YES (PlanningModeConfig) |
| Self-healing selectors | NO | YES (SelfHealingConfig) |
| Usage/token tracking | PARTIAL (manual count) | YES (atomic UsageStats) |
| Schema generation from examples | NO | YES (generate_schema) |
| Tool calling structured output | NO | YES (ToolCallingMode::Auto) |
| Concurrent action chains | NO | YES (DependencyGraph) |
| Long-term experience memory | NO | YES (memvid-rs feature) |

### Token Cost Impact

The HTML diff optimization alone (`HtmlDiffMode::Auto`) would reduce token costs by 50-70% on multi-round interactions. For axon_rust's extract pipeline doing 200+ pages, this compounds significantly.

---

## 8. Integration Proposal: cortex agent Command

### Design

Add a new `cortex agent` command that wraps `spider_agent::Agent` for research, extraction, and interactive web tasks.

### Cargo.toml Changes

```toml
# Add to axon_rust/Cargo.toml [dependencies]
spider_agent = { version = "2.45.20", path = "../spider/spider_agent", features = [
    "openai",          # LLM provider
    "search_serper",   # optional — gated on SERPER_API_KEY
    "search_brave",    # optional — gated on BRAVE_API_KEY
    "search_tavily",   # optional — gated on TAVILY_API_KEY
] }
```

Note: `chrome` feature requires a running Chrome instance — keep feature-gated, disabled by default.

### New Config Fields (`crates/core/config.rs`)

```rust
// Add to Config struct
pub agent_mode: AgentMode,
pub agent_max_concurrent: usize,      // default 5
pub agent_system_prompt: Option<String>,
pub agent_max_pages: usize,           // for research mode
pub agent_synthesize: bool,           // for research mode
pub serper_api_key: Option<String>,
pub brave_api_key: Option<String>,
pub tavily_api_key: Option<String>,

#[derive(Debug, Clone, Default)]
pub enum AgentMode {
    #[default]
    Extract,    // extract structured data from URL(s)
    Research,   // search + extract + synthesize
    Prompt,     // raw LLM prompt with web context
}
```

### New File: `crates/agent/mod.rs`

```rust
use spider_agent::{Agent, AgentConfig, ResearchOptions, SearchOptions, UsageLimits};
use std::sync::Arc;
use crate::crates::core::config::Config;

pub struct AgentRun {
    pub topic: String,
    pub mode: String,
    pub results: Vec<serde_json::Value>,
    pub summary: Option<String>,
    pub usage: AgentUsage,
}

pub struct AgentUsage {
    pub llm_calls: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub search_calls: u64,
    pub fetch_calls: u64,
}

pub async fn build_agent(cfg: &Config) -> Result<Arc<Agent>, Box<dyn std::error::Error>> {
    let api_key = &cfg.openai_api_key;
    let model = &cfg.openai_model;
    let base_url = &cfg.openai_base_url;

    let mut builder = Agent::builder();

    // LLM provider — always OpenAI-compatible (covers Ollama, LM Studio, etc.)
    builder = builder.with_openai_compatible(base_url, api_key, model);

    // Search provider — first configured key wins
    #[cfg(feature = "search_serper")]
    if let Some(ref key) = cfg.serper_api_key {
        builder = builder.with_search_serper(key);
    }
    #[cfg(feature = "search_brave")]
    if let Some(ref key) = cfg.brave_api_key {
        builder = builder.with_search_brave(key);
    }
    #[cfg(feature = "search_tavily")]
    if let Some(ref key) = cfg.tavily_api_key {
        builder = builder.with_search_tavily(key);
    }

    builder = builder.with_max_concurrent_llm_calls(cfg.agent_max_concurrent);

    Ok(Arc::new(builder.build()?))
}

pub async fn run_agent_extract(
    agent: &Agent,
    urls: &[String],
    prompt: &str,
) -> Result<AgentRun, Box<dyn std::error::Error>> {
    let mut handles = Vec::new();
    let agent = Arc::new(agent);  // NOTE: agent is already Arc in caller

    for url in urls {
        let agent = agent.clone();
        let url = url.clone();
        let prompt = prompt.to_string();
        handles.push(tokio::spawn(async move {
            match agent.fetch(&url).await {
                Ok(fetch) => agent.extract(&fetch.html, &prompt).await.ok(),
                Err(_) => None,
            }
        }));
    }

    let results: Vec<serde_json::Value> = futures::future::join_all(handles)
        .await
        .into_iter()
        .filter_map(|r| r.ok().flatten())
        .collect();

    let usage = agent.usage();
    Ok(AgentRun {
        topic: prompt.to_string(),
        mode: "extract".to_string(),
        results,
        summary: None,
        usage: AgentUsage {
            llm_calls: usage.llm_calls,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens(),
            search_calls: usage.search_calls,
            fetch_calls: usage.fetch_calls,
        },
    })
}

pub async fn run_agent_research(
    agent: &Agent,
    topic: &str,
    max_pages: usize,
    synthesize: bool,
) -> Result<AgentRun, Box<dyn std::error::Error>> {
    let opts = ResearchOptions::new()
        .with_max_pages(max_pages)
        .with_synthesize(synthesize);

    let research = agent.research(topic, opts).await?;
    let usage = agent.usage();

    let results: Vec<serde_json::Value> = research
        .extractions
        .iter()
        .map(|e| serde_json::json!({
            "url": e.url,
            "title": e.title,
            "data": e.extracted,
        }))
        .collect();

    Ok(AgentRun {
        topic: topic.to_string(),
        mode: "research".to_string(),
        results,
        summary: research.summary,
        usage: AgentUsage {
            llm_calls: usage.llm_calls,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens(),
            search_calls: usage.search_calls,
            fetch_calls: usage.fetch_calls,
        },
    })
}
```

### New CLI Command: `crates/cli/commands/agent.rs`

```rust
// cortex agent <query> [--mode extract|research|prompt]
//              [--urls url1,url2] [--max-pages 5] [--synthesize]
//              [--system-prompt "..."]

pub async fn run_agent(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let query = cfg.positional.first()
        .or(cfg.query.as_ref())
        .ok_or("agent requires a query or topic")?
        .to_string();

    let agent = build_agent(cfg).await?;

    match cfg.agent_mode {
        AgentMode::Extract => {
            let urls = parse_urls(cfg);
            if urls.is_empty() {
                return Err("agent extract requires --urls".into());
            }
            let run = run_agent_extract(&agent, &urls, &query).await?;
            print_agent_run(&run, cfg);
        }
        AgentMode::Research => {
            let run = run_agent_research(
                &agent, &query,
                cfg.agent_max_pages,
                cfg.agent_synthesize,
            ).await?;
            print_agent_run(&run, cfg);
        }
        AgentMode::Prompt => {
            let response = agent.prompt(vec![
                spider_agent::Message::user(query.clone())
            ]).await?;
            if cfg.json_output {
                println!("{}", serde_json::json!({"response": response}));
            } else {
                println!("{}", response);
            }
        }
    }

    Ok(())
}
```

### Worker Design

The agent command should support async job queuing like extract/crawl. Add `axon_agent_jobs` table:

```sql
CREATE TABLE IF NOT EXISTS axon_agent_jobs (
    id          UUID        PRIMARY KEY,
    mode        TEXT        NOT NULL,   -- 'extract', 'research', 'prompt'
    topic       TEXT        NOT NULL,
    status      TEXT        NOT NULL,   -- pending/running/completed/failed/canceled
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at  TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    error_text  TEXT,
    result_json JSONB,
    config_json JSONB NOT NULL
);
CREATE INDEX idx_axon_agent_jobs_status ON axon_agent_jobs(status);
```

Worker follows same AMQP pattern as existing `crawl_jobs.rs`/`embed_jobs.rs` using `jobs/common.rs` infrastructure (`make_pool`, `open_amqp_channel`, `claim_next_pending`, `mark_job_failed`, `enqueue_job`).

---

## 9. Priority Ranking

Ordered by value-to-effort ratio (highest first):

### Priority 1: Concurrent LLM Extraction via Arc<Agent>

**What**: Replace serial extract loop with `Arc<Agent>` + `tokio::spawn` fan-out.
**Why first**: Zero new dependencies. Uses `spider_agent` with `openai` feature only. Direct fix for the serial bottleneck in `remote_extract.rs`. Same endpoint, same model — just concurrent.
**Effort**: Small (refactor `remote_extract.rs`, add `spider_agent` dep with `openai` feature)
**Impact**: N-x throughput for extract jobs where N = `agent_max_concurrent`

### Priority 2: HTML Diff Optimization (50-70% Token Reduction)

**What**: Enable `RemoteMultimodalConfigs` with `HtmlDiffMode::Auto` for the extract pipeline.
**Why second**: Token costs drop 50-70% on multi-round interactions. Self-funding — saves more money than it costs to implement.
**Effort**: Medium (integrate `with_remote_multimodal()` into the crawl path instead of manual HTTP)
**Impact**: Massive cost reduction on large extract runs

### Priority 3: Schema-Based Structured Extraction

**What**: Add `--schema <json-file>` flag to `cortex extract`. Use `agent.extract_structured(html, schema)`.
**Why third**: Eliminates the "results" wrapper hack in current `collect_items()`. Clean typed output matching user-defined schemas.
**Effort**: Small (new CLI flag + pass schema to agent method)
**Impact**: More reliable extraction, user-controlled output shape

### Priority 4: Research Mode (Search + Extract + Synthesize)

**What**: `cortex agent research <topic>` using `agent.research(topic, opts)`.
**Why fourth**: Combines web search + extraction + synthesis into one command. Requires at least one search provider key (Serper/Brave/Tavily).
**Effort**: Medium (new `agent` command + search provider config + job worker)
**Impact**: New capability not previously possible in axon_rust

### Priority 5: Usage/Token Tracking in Jobs

**What**: Persist `UsageSnapshot` into `result_json` for all extract/agent jobs.
**Why fifth**: Token cost visibility for operational monitoring. Feeds into cost dashboards.
**Effort**: Small (plumb `agent.usage()` into job result serialization)
**Impact**: Operational observability

### Priority 6: Multi-Model Provider Abstraction

**What**: Add `--llm-provider [openai|gemini|anthropic]` flag. Use `LLMProvider` trait to swap providers.
**Why sixth**: Enables Ollama (already used), Gemini, Anthropic-direct without code changes.
**Effort**: Medium (config changes + provider selection logic)
**Impact**: Flexibility for different use cases (cost vs quality vs local)

### Priority 7: Session Memory for Research Chains

**What**: Use `AgentMemory` (DashMap) to persist state across extract/research steps within a job.
**Why seventh**: Enables stateful multi-step research (follow-up queries, context accumulation).
**Effort**: Small (add memory set/get calls around research steps)
**Impact**: Smarter, more coherent multi-page research results

### Priority 8: Interactive Browser Agent (not_a_robot capability)

**What**: `cortex agent automate <url> --task "..."` using `RemoteMultimodalConfigs` with Chrome.
**Why eighth**: Highest complexity, requires Chrome. But unlocks interactive SPAs, login flows, CAPTCHA bypass.
**Effort**: Large (chrome feature flag, BrowserContext setup, multi-round loop)
**Impact**: Access to pages that HTTP crawl can't reach

### Priority 9: Chrome Built-in AI (Gemini Nano)

**What**: `use_chrome_ai: true` flag in `RemoteMultimodalConfigs`. Zero API cost for extraction.
**Why last**: Requires Chrome Canary + on-device model download. Very narrow hardware requirement.
**Effort**: Small (config flag) but environment setup is large
**Impact**: Low API cost extraction when hardware requirements met

---

## Appendix: Key API Surfaces

### spider_agent Agent API (complete public surface)

```rust
// Builder
Agent::builder() -> AgentBuilder
AgentBuilder::with_openai(key, model) -> Self
AgentBuilder::with_openai_compatible(url, key, model) -> Self
AgentBuilder::with_spider_cloud(key) -> Self
AgentBuilder::with_search_serper/brave/bing/tavily(key) -> Self
AgentBuilder::with_browser(ctx) -> Self
AgentBuilder::with_max_concurrent_llm_calls(n) -> Self
AgentBuilder::with_system_prompt(s) -> Self
AgentBuilder::build() -> AgentResult<Agent>

// LLM
agent.prompt(messages: Vec<Message>) -> AgentResult<String>
agent.complete(messages) -> AgentResult<CompletionResponse>

// Extraction
agent.extract(html, prompt) -> AgentResult<serde_json::Value>
agent.extract_structured(html, schema) -> AgentResult<serde_json::Value>

// HTTP
agent.fetch(url) -> AgentResult<FetchResult>

// Search (feature-gated)
agent.search(query) -> AgentResult<SearchResults>
agent.search_with_options(query, opts) -> AgentResult<SearchResults>

// Research (feature-gated search)
agent.research(topic, ResearchOptions) -> AgentResult<ResearchResult>

// Memory (lock-free, always available)
agent.memory_get(key) -> Option<serde_json::Value>
agent.memory_set(key, value)
agent.memory_clear()

// Usage tracking (lock-free atomic counters)
agent.usage() -> UsageSnapshot
agent.reset_usage()

// Browser (feature-gated chrome)
agent.navigate(url), agent.click(selector), agent.type_text(selector, text)
agent.screenshot() -> Vec<u8>
agent.browser_html() -> String
agent.extract_page(prompt) -> serde_json::Value  // screenshot + LLM

// Custom tools
agent.register_custom_tool(tool)
agent.execute_custom_tool(name, path, query, body)
agent.execute_custom_tool_json(name, path, query, body)
```

### RemoteMultimodalConfigs API (spider core)

```rust
RemoteMultimodalConfigs::new(api_url, model) -> Self

// Fields
mm_config.api_key: Option<String>
mm_config.system_prompt: Option<String>
mm_config.user_message_extra: Option<String>
mm_config.cfg.extra_ai_data: bool       // enable AI extraction
mm_config.cfg.include_html: bool        // send HTML to LLM
mm_config.cfg.include_title: bool
mm_config.cfg.include_url: bool
mm_config.cfg.max_rounds: u8            // multi-round interaction
mm_config.cfg.request_json_object: bool // JSON mode
mm_config.cfg.extraction_prompt: Option<String>

// Integration into Website
Website::new(url).with_remote_multimodal(Some(mm_config)).build()

// Results arrive in subscribe channel
page.extra_remote_multimodal_data: Option<Vec<RemoteAiResult>>
result.content_output: serde_json::Value
result.usage: Option<AutomationUsage>  // token tracking
```

---

*Report complete. All code is read-only analysis from existing source files. No files were modified.*
