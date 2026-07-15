# Local Sources
Last Modified: 2026-07-15

Local paths are first-class sources in the unified pipeline.

## Inputs

Local source requests may point at files or directories. The adapter resolves
and classifies paths before acquisition, then applies local-source auth and
redaction rules.

## Behavior

Local files become source items, then documents. Directory sources may emit many
items but still belong to one source identity and one generation.

## Safety

Local hidden files, secrets, environment files, and denylisted paths must not be
indexed unless policy explicitly allows them. Auth is checked before dispatch
and again during execution.
