---
name: axon-snippets
description: Use when discovering, reviewing, installing, or running reusable Labby Code Mode snippets for Axon search, source indexing, research, extraction, monitoring, QA evidence, and the workflow skills planned for retirement.
---

# Axon Snippets

This skill routes the checked-in Labby Code Mode snippet sources in `snippets/`. They cover every existing Axon plugin skill outside `install-axon` and `using-axon`. The old skills remain available during migration. A snippet performs its stated Axon call and returns an evidence receipt; it does not produce the entire human-facing report or replace browser interaction where that is needed.

## Workflow

Choose a catalog entry, inspect its source and live tool schema, validate it, then install or run it only for a task that calls for that action.

## Discover and run

1. Choose a snippet below. Open its Markdown file to inspect its inputs, action, limits, and side effects.
2. Search the live Labby Code Mode catalog for Axon and describe the returned tool before use. These sources were authored for `Axon::axon`; if the connected gateway differs, revise the source and revalidate before installation.
3. Resolve this skill's directory as `SKILL_DIR`; validate a source without saving: `labby snippet validate <name> --file "$SKILL_DIR/snippets/<name>.md"`.
4. To install into the user's local Labby home, run `labby snippet add <name> --file "$SKILL_DIR/snippets/<name>.md"`. Inspect existing snippets with `labby snippet list --json` and `labby snippet get <name> --json` before replacing one; `--force` is for an intentional replacement.
5. Run `labby snippet test <name> --param key=value` only when the real upstream call and its side effects are authorized. Use `labby snippet run <name> --param key=value` for normal execution. MCP/API callers use `snippets.exec` with `{name,params}` and require Labby admin scope. `codemode.run(name,input)` is available within an authorized Code Mode run.

All snippets declare only `Axon::axon` in `tools`. This narrows an existing caller scope; it never grants access. Most return a bounded preview and artifact receipt. `source`, `scrape`, `search`, `research`, and `extract` can acquire or index external material. `screenshot` may contact the target site. Run only against sources the task authorizes. `monitor` lists watches and does not create one.

## Snippet catalog

| Snippet | What it does | When to use |
| --- | --- | --- |
| `axon-cli` | Check Axon CLI-facing runtime readiness through MCP | Use before CLI automation to inspect provider health; shell-specific commands remain in using-axon. |
| `axon-company-directories` | Extract company records from an authorized directory | Use for a known public or authorized directory page; verify extracted fields against the source. |
| `axon-competitive-intel` | Search and research competitor changes with source receipts | Use for pricing, feature, or launch comparisons; check source dates before reporting changes. |
| `axon-crawl` | Index a bounded documentation site | Use when an entire site or docs section should enter the index. This changes the Axon corpus. |
| `axon-dashboard-reporting` | Capture and summarize a public dashboard page | Use for a page capture; authentication and interaction require a browser outside this snippet. |
| `axon-deep-research` | Search and synthesize sources for a complex research question | Use for a complex question; inspect sources and build the final report outside the snippet. |
| `axon-demo-walkthrough` | Capture and summarize a product demo page | Use for visual evidence from one page; use browser automation for navigation and interaction. |
| `axon-download` | Capture one page to an Axon artifact | Use for a known page. Follow the returned artifact path for saved content. |
| `axon-extract` | Extract requested structured fields from one page | Use when a page is known and field-level data is needed; verify values before export. |
| `axon-knowledge-base` | Build an indexed documentation corpus | Use for corpus creation or refresh. This changes the Axon index. |
| `axon-knowledge-ingest` | Ingest an authorized knowledge portal | Use for an accessible portal section; browser login and URL discovery happen outside this snippet. |
| `axon-lead-gen` | Discover and research public lead sources | Use for initial public-source discovery; qualify each lead before CRM export. |
| `axon-lead-research` | Search and research a company before a meeting | Use for a source-backed briefing; check people and recency manually. |
| `axon-map` | Discover URLs on a site | Use before selecting pages to index or extract; map does not prove every page is reachable. |
| `axon-market-research` | Search and research market evidence | Use for cited market evidence; distinguish reported facts from estimates. |
| `axon-monitor` | List configured Axon source watches | Use to inspect existing watches. Creating, changing, or deleting a watch needs separate review. |
| `axon-qa` | Map and screenshot a live site for QA evidence | Use for one visual checkpoint; interactive QA requires a browser. |
| `axon-research-papers` | Search and research papers or reports | Use for source discovery and synthesis; verify papers, methods, and citations. |
| `axon-scrape` | Capture a known page through Axon | Use for single-page content. Check the returned artifact or preview for completeness. |
| `axon-search` | Search current web sources | Use for current source discovery; Axon may enqueue indexing as a side effect. |
| `axon-seo-audit` | Map and scrape a site for an SEO review | Use for crawlable URL inventory; inspect metadata and page quality separately. |
| `axon-shop` | Search and research products and reviews | Use for product comparison evidence; verify current price, stock, and seller. |
| `axon-website-design-clone` | Capture brand, content, and screenshot evidence for a design brief | Use for colors, typography, and voice; screenshots and visual verification complete a DESIGN.md. |
| `axon-workflows` | Query and ask the Axon corpus for a grounded answer | Use for a broad Axon-backed task when no more specific snippet matches. |

## Coverage and limits

The 24 entries map one to one to the existing skills planned for retirement. Simple action skills map to one action; outcome-oriented skills gather an Axon evidence input for a later report. `dashboard-reporting`, `demo-walkthrough`, `qa`, and `website-design-clone` require browser or visual review for interactive claims. `company-directories` and `extract` require source-level field checks. `monitor` only inspects existing watches; scheduling and destructive watch operations remain deliberate manual steps. Keep the legacy skills until the user confirms the replacements meet their workflows.

The source files are portable authoring artifacts, not auto-installed Labby built-ins. Installing them modifies the selected Labby home; publishing the Axon plugin alone does not deploy them into a gateway. See `using-axon` for the live action map and `install-axon` for first-run setup.
