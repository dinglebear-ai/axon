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
