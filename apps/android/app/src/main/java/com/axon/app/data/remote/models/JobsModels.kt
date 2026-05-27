package com.axon.app.data.remote.models

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

/** ServiceJob — common shape across /v1/{crawl,embed,extract,ingest}/list and /{id}. */
@Serializable
data class ServiceJob(
    val id: String = "",
    val status: String = "",
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("updated_at") val updatedAt: String? = null,
    @SerialName("started_at") val startedAt: String? = null,
    @SerialName("finished_at") val finishedAt: String? = null,
    @SerialName("error_text") val errorText: String? = null,
    val url: String? = null,                                       // crawl
    @SerialName("source_type") val sourceType: String? = null,      // ingest
    val target: String? = null,                                     // ingest/embed/extract
    @SerialName("result_json") val resultJson: JsonElement? = null, // locked: JsonElement, not JsonObject
    @SerialName("config_json") val configJson: JsonElement? = null,
)

/** GET /v1/status response — aggregated job counts. */
@Serializable
data class StatusSummary(
    val payload: JsonElement,
)

/** POST /v1/{kind}/{id}/cancel response. */
@Serializable
data class CancelResponse(
    val canceled: Boolean = false,
)
