# Android Surface
Last Modified: 2026-07-15

Android consumes the same REST DTOs and OpenAPI contracts as other clients.

## Rules

- Do not create Android-only request semantics.
- Generated client routes must be covered by OpenAPI.
- Auth and destructive-operation policy are server-owned.
- Source, job, query, ask, and memory flows use shared DTOs.

## Verification

Run the Android API contract check whenever REST routes or generated clients
change.
