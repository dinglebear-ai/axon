# Axon Capture Recipes

Use these recipes when a workflow needs concrete Axon evidence.

## Command Selection

| Need | Prefer |
|---|---|
| Discover URLs | `axon map <url> --json` |
| Crawl a docs/site section | `axon crawl <url> --output-dir <dir> --wait true` |
| Fetch selected pages | `axon scrape <url> --json` |
| Search current web sources | `axon search "<query>" --json` |
| Search plus cited synthesis | `axon research "<query>" --json` |
| Structured fields | `axon extract <url> --wait true --json` plus an explicit task/output schema |
| Brand/design signals | `axon brand <url> --json` |
| Rendered page evidence | `axon --output-dir <dir> screenshot <url> --json` |
| Compare URL versions | `axon diff <url-a> <url-b> --json` |
| Monitor recurring changes | `axon watch create ...`, then `watch run-now` and `watch history` |
| Reuse indexed material | `axon ask "<question>"` or `axon query "<text>" --json` |

## Browser Boundary

Axon can render pages through Chrome and run crawl automation scripts, but it is
not a general saved-profile browser operator. Use host browser automation for
interactive login, forms, carts, dashboards, responsive checks, console/network
capture, and manual navigation. Feed discovered URLs or exported artifacts back
to Axon for scrape, extract, ask, query, or summarize.

## Evidence Fields

For every claim or row in a workflow deliverable, preserve:

- source URL or artifact path
- extracted/observed timestamp
- command or browser action used
- confidence and limitations
- auth/access state when relevant
- fields observed vs inferred
