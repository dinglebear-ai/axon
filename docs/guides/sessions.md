# Session Sources
Last Modified: 2026-07-15

Session transcripts are source inputs for Claude, Codex, Gemini, and related
agent logs.

## Behavior

The session adapter reads transcript exports, normalizes turns, preserves
session metadata, and prepares searchable documents. Limits apply before
diffing so very large transcript folders remain bounded.

## Identity

Session documents should preserve source system, session id, path or export
origin, turn ids, timestamps, and role metadata where available.

## Use Cases

- recover implementation context
- search prior decisions
- attach agent evidence to project work
