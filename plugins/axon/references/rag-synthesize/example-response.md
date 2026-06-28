# Example Response

This documents the expected output format for the rag-synthesize synthesis prompt.
Used by human maintainers — this file is NOT read by the Rust loader or passed to the LLM.

## Context the LLM receives

```
Sources:
## Top Chunk [S1]: https://docs.example.com/hooks

Hooks in Example Tool are user-defined handlers that execute automatically at specific,
deterministic points in the lifecycle...

---

## Source Document [S2]: https://docs.example.com/hooks-reference

### Lifecycle Events

SessionStart — fires when a new session begins.
PostToolUse — fires after every tool call completes.
UserPromptSubmit — fires when the user submits a prompt.
...

---

## Supplemental Chunk [S3]: https://docs.example.com/plugins

Plugins can bundle hooks in a hooks/hooks.json file...
```

## Expected output: exhaustive query ("tell me ALL about hooks")

```markdown
Hooks are user-defined handlers that execute automatically at fixed lifecycle points,
not at the model's discretion [S1].

### Configuration

Each hook configuration has three parts [S1]:
- **Event**: the lifecycle point where the hook fires
- **Matcher**: filters that determine which events trigger it
- **Handler**: the action to execute

### Lifecycle Events

The following events are available [S2]:

- **SessionStart** — fires when a new session begins
- **PostToolUse** — fires after every tool call completes
- **UserPromptSubmit** — fires when the user submits a prompt

### Plugin Integration

Plugins can bundle hooks in a `hooks/hooks.json` file [S3].

Gaps: The sources do not cover hook handler types, error handling behavior, or
configuration schema validation.

## Sources
[S1] https://docs.example.com/hooks
[S2] https://docs.example.com/hooks-reference
[S3] https://docs.example.com/plugins
```

## Expected output: focused query ("what is PostToolUse?")

```markdown
`PostToolUse` is a lifecycle event that fires after every tool call completes [S2].
It can be used to inspect or react to tool results without modifying the tool call itself.

## Sources
[S2] https://docs.example.com/hooks-reference
```

## Expected output: no relevant context

```
The indexed sources do not contain information about hook timeout configuration.
To index relevant content, consider: (1) the hooks configuration reference at
docs.example.com/hooks/config, (2) the CLI flags reference for timeout settings,
or (3) the GitHub issues tracker for timeout-related discussions.
```

## Citation rules

- `[S1]` goes inline after the claim, before punctuation: `...at fixed points [S1].`
- Multiple sources: `[S2][S4]` with no space between
- Lists from one source: cite once at the end of the last item, or once per group header
- Lists from mixed sources: cite each item individually
