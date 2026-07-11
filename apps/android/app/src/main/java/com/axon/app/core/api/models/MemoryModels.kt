package com.axon.app.core.api.models

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Wire request for `POST /v1/memories` (remember) and `POST
 * /v1/memories/search` (search). Mirrors the server's flat
 * `RestMemoryRequest` contract
 * (`crates/axon-services/src/client_contract/memory.rs`, `#[serde(deny_unknown_fields)]`)
 * — only the fields relevant to a mobile client are exposed here. All fields
 * are optional and kotlinx.serialization does not encode defaults, so only
 * the fields a caller actually sets are serialized; the server's
 * `deny_unknown_fields` gate never sees an unexpected key.
 */
@Serializable
data class MemoryRequestDto(
    @SerialName("memory_type") val memoryType: String? = null,
    val title: String? = null,
    val body: String? = null,
    val query: String? = null,
    val project: String? = null,
    val repo: String? = null,
    val file: String? = null,
    @SerialName("scope_kind") val scopeKind: String? = null,
    @SerialName("scope_value") val scopeValue: String? = null,
    val confidence: Double? = null,
    val salience: Double? = null,
    val limit: Int? = null,
)
