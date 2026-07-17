package com.axon.app.core.api.models

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

/** POST /v1/extract request body. */
@Serializable
data class ExtractRequest(
    val urls: List<String>,
    val prompt: String? = null,
    @SerialName("max_pages") val maxPages: Int? = null,
    @SerialName("render_mode") val renderMode: String? = null,
    val embed: Boolean? = null,
    val headers: List<String> = emptyList(),
)

/** Source indexing request submitted through POST /v1/sources. */
@Serializable
data class SourceIndexRequest(
    val input: String,
    val collection: String? = null,
)

/** UI-facing projection of a unified job row. */
@Serializable
data class ServiceJob(
    val id: String = "",
    val status: String = "",
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("updated_at") val updatedAt: String? = null,
    @SerialName("started_at") val startedAt: String? = null,
    @SerialName("finished_at") val finishedAt: String? = null,
    @SerialName("error_text") val errorText: String? = null,
    val url: String? = null,
    @SerialName("source_kind") val sourceKind: String? = null,
    val target: String? = null,
    @SerialName("progress_json") val progressJson: JsonElement? = null,
    @SerialName("result_json") val resultJson: JsonElement? = null, // locked: JsonElement, not JsonObject
    @SerialName("config_json") val configJson: JsonElement? = null,
)

/** GET /v1/jobs response — generic source-pipeline job page. */
@Serializable
data class JobSummaryPage(
    val items: List<UnifiedJobSummary> = emptyList(),
    @SerialName("next_cursor") val nextCursor: String? = null,
    val limit: Int = 0,
    val total: Long? = null,
)

/** JobSummary from the unified `/v1/jobs` surface. */
@Serializable
data class UnifiedJobSummary(
    @SerialName("job_id") val jobId: String = "",
    val kind: String? = null,
    val status: String = "",
    val phase: String? = null,
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("updated_at") val updatedAt: String? = null,
    val counts: JsonElement? = null,
    @SerialName("last_error") val lastError: JsonElement? = null,
)

fun UnifiedJobSummary.toServiceJob(): ServiceJob =
    ServiceJob(
        id = jobId,
        status = status,
        createdAt = createdAt,
        updatedAt = updatedAt,
        errorText = lastError?.toString(),
        progressJson = counts,
    )

/** GET /v1/status response — aggregated job counts. */
@Serializable
data class StatusSummary(
    val payload: JsonElement,
)

/** Transport-neutral cancellation result used by the Android UI. */
@Serializable
data class CancelResponse(
    val canceled: Boolean = false,
)

/** POST /v1/jobs/{id}/cancel response. */
@Serializable
data class UnifiedJobCancelResult(
    @SerialName("job_id") val jobId: String = "",
    val status: String = "",
)

/** GET /v1/watches response envelope. */
@Serializable
data class WatchListResponse(
    val items: List<WatchDef> = emptyList(),
    val watches: List<WatchDef> = emptyList(),
) {
    val allWatches: List<WatchDef>
        get() = items.ifEmpty { watches }
}

/** Watch definition shape returned by the REST watch list endpoint. */
@Serializable
data class WatchDef(
    @SerialName("watch_id") val watchId: String = "",
    val id: String = "",
    val name: String = "",
    @SerialName("source_id") val sourceId: String = "",
    @SerialName("task_type") val taskType: String = "",
    @SerialName("task_payload") val taskPayload: JsonElement? = null,
    val schedule: WatchSchedule = WatchSchedule(),
    @SerialName("every_seconds") val legacyEverySeconds: Long = 0L,
    val enabled: Boolean = false,
    @SerialName("next_run_at") val nextRunAt: String? = null,
    @SerialName("lease_expires_at") val leaseExpiresAt: String? = null,
    @SerialName("last_run_at") val lastRunAt: String? = null,
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("updated_at") val updatedAt: String? = null,
) {
    val displayId: String
        get() = id.ifBlank { watchId }

    val displayName: String
        get() = name.ifBlank { sourceId.ifBlank { displayId } }

    val displayTaskType: String
        get() = taskType.ifBlank { "watch" }

    val everySeconds: Long
        get() = schedule.everySeconds.takeIf { it > 0 } ?: legacyEverySeconds
}

@Serializable
data class WatchSchedule(
    @SerialName("every_seconds") val everySeconds: Long = 0L,
)
