package com.axon.app.data.remote.models

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** POST /v1/summarize request — mirrors `RestSummarizeRequest`. */
@Serializable
data class SummarizeRequest(
    val url: String? = null,
    val urls: List<String>? = null,
    @SerialName("render_mode") val renderMode: String? = null,   // "http" | "chrome" | "auto-switch"
    @SerialName("root_selector") val rootSelector: String? = null,
    @SerialName("exclude_selector") val excludeSelector: String? = null,
    val headers: List<String> = emptyList(),                       // "Key: Value" strings
    val collection: String? = null,
)

/** POST /v1/summarize response — mirrors `SummarizeResult`. */
@Serializable
data class SummarizeResponse(
    val urls: List<String> = emptyList(),
    @SerialName("context_chars") val contextChars: Long = 0,
    @SerialName("context_truncated") val contextTruncated: Boolean = false,
    val summary: String = "",
)
