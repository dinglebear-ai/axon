# Source and durable-job acceptance

`orchestrator.py` is the real-product entry point for bead `.6`. It requires an
executable Axon binary, healthy Qdrant/TEI/Chrome providers, the HTTP server,
and an MCP server address known to `mcporter`. A fake executable is used only
by `test_orchestrator.py` to test harness protocol and fail-closed behavior; it
is never an acceptance substitute.

The CI composition bead (`.12`) owns starting these services and exports the
arguments consumed by `RealAxonSourceJobTests`. In particular, its controllable
provider doubles must expose three independent blocking scenarios:

- acquisition: the source request blocks in exact `fetching` phase until its release URL is
  called;
- embedding: the configured TEI double blocks in exact `embedding` phase until release;
- publication: the Qdrant boundary blocks in exact `publishing` phase after partial publication until
  release.

Axon job events are the synchronization primitive. The harness never sleeps and
guesses that a stage was reached: it waits for the exact public phase, persists
cancellation, then releases the provider. The publication case requires
non-empty `side_effects` and `cleanup_debt_ids` from `JobCancelResult`, then
waits for resolution events naming every exact debt ID.

The real runner additionally proves the same durable job identity and status
over CLI, HTTP `/v1/jobs/{id}`, and MCP `action=jobs,subaction=get`; indexes and
retrieves the canonical corpus; checks ledger manifest/item state; executes a
Chrome-rendered acquisition; invokes `jobs recover` from a fresh Axon process;
and verifies retry stability through CLI stats/collection evidence and a
source-filtered Qdrant scroll of exact point IDs and generation payloads. Every
returned job, source, artifact, reservation, debt, and decoded MCP task ID
is registered in the run manifest. Bead `.15` owns final teardown and residual
audit of those registrations.
