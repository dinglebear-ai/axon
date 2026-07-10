package com.axon.app.data.remote.models

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

/**
 * SSE payload emitted by `GET /v1/jobs/{id}/stream`. Mirrors the server's
 * flat `StreamEvent` envelope (`crates/axon-api/src/source/status.rs`) —
 * `kind` discriminates the shape carried in [data]: a `SourceProgressEvent`
 * for `"progress"`, a `{"text": ...}` object for `"token"`, the
 * route-specific result DTO for `"final"`, and so on. See
 * `docs/pipeline-unification/surfaces/android-contract.md` "Streaming
 * Contract" for the per-kind UI handling table.
 */
@Serializable
data class JobStreamEventDto(
    @SerialName("event_id") val eventId: String = "",
    val kind: String = "",
    val sequence: Long = 0,
    val timestamp: String? = null,
    @SerialName("job_id") val jobId: String? = null,
    @SerialName("request_id") val requestId: String? = null,
    val data: JsonElement? = null,
    val warning: JsonElement? = null,
    val error: JsonElement? = null,
)
