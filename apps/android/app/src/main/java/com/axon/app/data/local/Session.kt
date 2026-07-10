package com.axon.app.data.local

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonPrimitive

@Entity(
    tableName = "sessions",
    indices = [Index(value = ["pinned_at", "updated_at"])]
)
data class Session(
    @PrimaryKey val id: String,
    val title: String,
    @ColumnInfo(name = "first_message_preview") val firstMessagePreview: String,
    @ColumnInfo(name = "turn_count") val turnCount: Int = 0,
    @ColumnInfo(name = "injected_op_count") val injectedOpCount: Int = 0,
    @ColumnInfo(name = "created_at") val createdAt: Long,
    @ColumnInfo(name = "updated_at") val updatedAt: Long,
    @ColumnInfo(name = "pinned_at") val pinnedAt: Long? = null,
    /**
     * Mobile Session Model fields required by
     * `docs/pipeline-unification/surfaces/android-contract.md` ("Mobile
     * Session Model": `status`, `source_refs`, `draft`, `sync_version`; `id`
     * above already serves as the contract's `session_id`). The server's
     * `MobileSession` DTO (`crates/axon-services/src/mobile_sessions.rs`)
     * does not carry these fields yet — they are cached client-side only
     * until that axon-api model is upgraded to match (joint deferred item;
     * see android-contract.md audit U3-08/U3-09). `source_refs` is stored as
     * a JSON-encoded string column — see [sourceRefs]/[encodeSourceRefs] —
     * rather than adding Room `TypeConverter` infrastructure for one column.
     */
    @ColumnInfo(name = "status", defaultValue = "'active'") val status: String = "active",
    @ColumnInfo(name = "source_refs", defaultValue = "'[]'") val sourceRefsJson: String = "[]",
    @ColumnInfo(name = "draft") val draft: String? = null,
    @ColumnInfo(name = "sync_version") val syncVersion: Long? = null,
) {
    companion object {
        fun encodeSourceRefs(refs: List<String>): String =
            JsonArray(refs.map { JsonPrimitive(it) }).toString()

        fun decodeSourceRefs(json: String): List<String> = runCatching {
            (Json.parseToJsonElement(json) as? JsonArray)
                ?.map { it.jsonPrimitive.content }
                ?: emptyList()
        }.getOrDefault(emptyList())
    }
}

/** Decoded view of [Session.sourceRefsJson] — see [Session.encodeSourceRefs]. */
val Session.sourceRefs: List<String> get() = Session.decodeSourceRefs(sourceRefsJson)
