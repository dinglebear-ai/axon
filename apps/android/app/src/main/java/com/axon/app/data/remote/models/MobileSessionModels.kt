package com.axon.app.data.remote.models

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject

@Serializable
data class MobileChatItemDto(
    val kind: String,
    val text: String? = null,
    val payload: JsonObject = buildJsonObject {},
    val timestamp: Long,
)

@Serializable
data class MobileSessionDto(
    val id: String,
    val title: String,
    @SerialName("first_message_preview") val firstMessagePreview: String,
    @SerialName("turn_count") val turnCount: Int = 0,
    @SerialName("injected_op_count") val injectedOpCount: Int = 0,
    @SerialName("created_at") val createdAt: Long,
    @SerialName("updated_at") val updatedAt: Long,
    @SerialName("pinned_at") val pinnedAt: Long? = null,
    val items: List<MobileChatItemDto> = emptyList(),
    /**
     * Mobile Session Model fields from android-contract.md (`id` above
     * already serves as the contract's `session_id`). The server's
     * `MobileSession` DTO (`crates/axon-services/src/mobile_sessions.rs`) and
     * the generated OpenAPI `MobileSession` model do not carry these fields
     * yet, so a round trip through `upsertMobileSession`/`listMobileSessions`
     * (which go through the generated client) drops them — they only survive
     * in the local Room cache today (see `Session.kt`). Wiring server
     * persistence is a joint deferred item pending the axon-api model update.
     */
    val status: String = "active",
    @SerialName("source_refs") val sourceRefs: List<String> = emptyList(),
    val draft: String? = null,
    @SerialName("sync_version") val syncVersion: Long? = null,
)

@Serializable
data class MobileSessionListResponse(
    val sessions: List<MobileSessionDto> = emptyList(),
)

@Serializable
data class MobileSessionDetailResponse(
    val session: MobileSessionDto,
)

@Serializable
data class UpsertMobileSessionRequest(
    val session: MobileSessionDto,
)

@Serializable
data class UpsertMobileSessionResponse(
    val ok: Boolean,
    val session: MobileSessionDto,
)

@Serializable
data class DeleteMobileSessionResponse(
    val ok: Boolean,
)
