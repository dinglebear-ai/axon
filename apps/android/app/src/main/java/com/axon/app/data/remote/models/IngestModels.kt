package com.axon.app.data.remote.models

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** POST /v1/ingest request — mirrors `RestIngestRequest`. */
@Serializable
data class IngestRequest(
    @SerialName("source_type") val sourceType: String,   // "github"|"gitlab"|"gitea"|"git"|"reddit"|"youtube"
    val target: String? = null,
    @SerialName("include_source") val includeSource: Boolean? = null,
)

/** AcceptedJob — 202 response from POST /v1/ingest. */
@Serializable
data class AcceptedJob(
    @SerialName("job_id") val jobId: String,
    val status: String = "pending",
    @SerialName("status_url") val statusUrl: String? = null,
)
