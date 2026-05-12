---
status: ok
subtopic: skill-file-conventions
---

# Skill File Conventions as Editable Prompt Sources

## Is per-request file read a real performance concern?

**No, not at axon's request volume.** Evidence from three angles:

1. **File I/O at OS level**: A single `fs::read_to_string` of a ~2KB prompt file completes in under 100 microseconds on Linux with a warm page cache. The LLM inference round-trip dominates at 2,000–15,000ms. The file read is below measurement noise.

2. **Anthropic prompt caching economics**: Cache writes cost 25% premium over base; cache reads cost 10% of base (90% discount). Break-even at 2+ cache hits per prefix. The system prompt is the canonical stable prefix for Anthropic's prompt caching. If the prompt text changes on every request (due to per-request file reads with dynamic content), the cache prefix breaks and every request pays full input-token cost. Therefore: **load the prompt at startup, cache it, and serve it unchanged per request** for maximum caching benefit. If the file changes, reload on SIGHUP or at next process start.

3. **Claude Code's production pattern (Anthropic)**: "Built entirely around keeping the cache hot, loading its system prompt, tool definitions, and project files... paying this cost once since every token is new at startup." System prompts that mutate per-request (e.g., a timestamp injected into the system prompt) are explicitly identified as "real examples of what has broken caches in production."

**Verdict**: Per-request file read is not a performance concern for latency, but it will break Anthropic prompt caching if it results in any per-request variation in the prompt text. Read once at startup, or read on change detection (mtime check with in-memory cache).

## Recommended pattern: startup-load with fallback constant

The correct structure for axon's use case:

```
const HARDCODED_ASK_PROMPT: &str = r###"...current prompt text..."###;

fn load_ask_prompt(prompt_path: Option<&Path>) -> &'static str {
    // Returns a &'static str — either the constant or a leaked Box<str>
    // Leaking is acceptable for a process-lifetime string read once at startup.
    if let Some(path) = prompt_path {
        if let Ok(text) = std::fs::read_to_string(path) {
            let s: &'static str = Box::leak(text.into_boxed_str());
            return s;
        }
        // File present but unreadable: log warn, fall through to constant
    }
    HARDCODED_ASK_PROMPT
}
```

Key properties:
- **Fallback constant is always present**: if no file path is configured or the file is missing, the hardcoded constant is used. No runtime failure, no empty prompt.
- **File is loaded once at startup** (when ServiceContext is constructed), not per request. The resulting `&'static str` is stored in the config or a LazyLock.
- **No per-request allocation**: serving requests uses a reference to the startup-loaded string.
- **Prompt caching friendly**: the prompt text is stable across requests, so Anthropic's prompt cache prefix never breaks.

## How the fallback constant should be structured

The hardcoded constant (`HARDCODED_ASK_PROMPT`) should be:
- The known-good prompt at the time of the last release
- Embedded directly in the source file (as it currently is in `streaming.rs:7-32`) so it is always available even in a fresh deployment with no config file
- Clearly documented with a comment that the live file path can override it
- Version-controlled: when the prompt is edited in the external file and validated, the constant should be updated to match so a fresh deployment is not degraded

## Live file path conventions

From the Claude Skills ecosystem (observed in this project's `.claude/plugins/cache/`): the live file is a SKILL.md with YAML frontmatter for metadata and a markdown body for the prompt text. The body is loaded directly as the instruction surface.

For axon, the cleanest parallel:
- Live prompt file: `~/.axon/prompts/ask.txt` (or `ask.md`, consistent with AXON_DATA_DIR)
- Configured via env var: `AXON_ASK_PROMPT_PATH` (optional; if absent, use the hardcoded constant)
- The file format can be plain text with no frontmatter (simpler) or markdown (allows comments and section headers as cognitive scaffolding for the human editing the file)

Recommendation: plain text. Frontmatter parsing adds a dependency and the file has one purpose (the prompt text). Comments can live above the constant in the source file.

## Are there performance implications of file reads per request?

Yes, one: indirect. If you read the file per request and any system generates dynamic per-request content (even a trailing newline difference), the Anthropic KV cache for the system prompt prefix will never hit. At $3/MTok input and 1,000 ask requests/day with a 2,000-token system prompt, a cold-cache vs. hot-cache difference is ~$0.054/day with caching, ~$6/day without — approximately 100x cost difference at scale. For low-volume deployments this is immaterial; it matters for shared deployments.

## Sources
- [Anthropic Prompt Caching (Redis/blog)](https://redis.io/blog/what-is-prompt-caching/)
- [Claude Code prompt-caching production pattern](https://projectdiscovery.io/blog/how-we-cut-llm-cost-with-prompt-caching)
- [Don't Break the Cache (arxiv 2601.06007)](https://arxiv.org/html/2601.06007v2)
- [Prompt Caching Infrastructure Guide (introl.com)](https://introl.com/blog/prompt-caching-infrastructure-llm-cost-latency-reduction-guide-2025)
- [LLM Prompt Caching (Medium/Hannecke)](https://medium.com/@michael.hannecke/llm-prompt-caching-what-you-should-know-2665d76d3d8d)
