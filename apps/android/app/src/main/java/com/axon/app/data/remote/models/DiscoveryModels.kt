package com.axon.app.data.remote.models

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

/** GET /v1/doctor — payload is service-connectivity check results. */
@Serializable
data class DoctorResponse(val payload: JsonElement)

/** POST /v1/suggest. */
@Serializable
data class SuggestRequest(
    val focus: String? = null,
    val collection: String? = null,
)

@Serializable
data class SuggestHit(
    val url: String = "",
    val reason: String? = null,
)

@Serializable
data class SuggestResponse(
    val urls: List<SuggestHit> = emptyList(),
    val suggestions: List<SuggestHit> = emptyList(),
)

/** GET /v1/domains. */
@Serializable
data class DomainFacet(
    val domain: String = "",
    val vectors: Long = 0,
)

@Serializable
data class DomainsResponse(
    val domains: List<DomainFacet> = emptyList(),
    val limit: Long = 0,
    val offset: Long = 0,
)
