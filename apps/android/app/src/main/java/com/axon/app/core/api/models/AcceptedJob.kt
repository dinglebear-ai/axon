package com.axon.app.core.api.models

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Shared accepted-job projection used by async extract/embed helpers. */
@Serializable
data class AcceptedJob(
    @SerialName("job_id") val jobId: String,
    val status: String = "pending",
    @SerialName("status_url") val statusUrl: String? = null,
)
