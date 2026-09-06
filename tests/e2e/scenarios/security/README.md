# Security-negative E2E pack

`execute.py` consumes observations from the real CLI/MCP/HTTP adapters. It
fails unless every auth surface and every SSRF alternate form was executed,
forbidden destinations retained zero connections, provider bypass attempts
were rejected, and all evidence artifacts pass transformed-canary scanning.

The runner never opens forbidden URLs itself. Hermetic adapters route requests
through Axon inside deny-external containment and report owned sentinel counters.
Live adapters may use only owned sinks. Destructive execution is separately
guarded by `../admin/destructive_guard.py`; it re-fetches the canonical plan
immediately before each run-owned deletion.
