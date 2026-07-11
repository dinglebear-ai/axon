package com.axon.app.core.api

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonElement
import com.axon.app.core.api.models.MemoryRequestDto

// ── Memory ────────────────────────────────────────────────────────────────
// android-contract.md "Required API Surface" — memory row. Response shapes
// are dispatched server-side as untyped `serde_json::Value` (see
// `handlers::memory_routes`), so results decode generically as
// [JsonElement] rather than binding to one Kotlin shape per subaction.
//
// Extension functions (not class members) so AxonClient.kt stays under the
// repo's monolith line cap. Public call sites are unaffected — `client.
// rememberMemory(...)` resolves identically whether this is a member or an
// extension function.

/** POST /v1/memories — create a durable memory (`memory.remember`). */
suspend fun AxonClient.rememberMemory(req: MemoryRequestDto): Result<JsonElement> = withContext(Dispatchers.IO) {
    post(openApiRoute("POST", "/v1/memories"), req)
}

/** POST /v1/memories/search — semantic memory recall (`memory.search`). */
suspend fun AxonClient.searchMemories(req: MemoryRequestDto): Result<JsonElement> = withContext(Dispatchers.IO) {
    post(openApiRoute("POST", "/v1/memories/search"), req)
}

/**
 * POST /v1/memories/context — assemble a memory context bundle for a
 * project/repo/file scope (`memory.context`). Distinct route from
 * [searchMemories]; both accept the same flat [MemoryRequestDto] body.
 */
suspend fun AxonClient.memoryContext(req: MemoryRequestDto): Result<JsonElement> = withContext(Dispatchers.IO) {
    post(openApiRoute("POST", "/v1/memories/context"), req)
}

/**
 * GET /v1/memories/{memory_id} — fetch a single memory (`memory.show`).
 * There is no server-side list route (`/v1/memories` only accepts POST
 * for `remember`) — [searchMemories]/[memoryContext] are the read-many
 * surfaces; this is read-one by id.
 */
suspend fun AxonClient.getMemory(memoryId: String): Result<JsonElement> = withContext(Dispatchers.IO) {
    get(openApiRoute("GET", "/v1/memories/{memory_id}", "/v1/memories/${encodePathSegment(memoryId)}"))
}

/** DELETE /v1/memories/{memory_id} — forget a memory (`memory.forget`). */
suspend fun AxonClient.deleteMemory(memoryId: String): Result<JsonElement> = withContext(Dispatchers.IO) {
    delete(openApiRoute("DELETE", "/v1/memories/{memory_id}", "/v1/memories/${encodePathSegment(memoryId)}"))
}
