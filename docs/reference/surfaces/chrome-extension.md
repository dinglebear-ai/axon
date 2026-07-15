# Chrome Extension Surface
Last Modified: 2026-07-15

The Chrome extension is a client surface over the shared Axon API.

## Responsibilities

- capture browser context when explicitly requested
- submit source or ask/query requests through shared DTOs
- display server-provided progress and results
- respect auth and user consent boundaries

## Rule

The extension must not implement a private ingestion pipeline. Browser pages
become source requests or explicit artifacts.
