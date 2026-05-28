package com.axon.app.data.local

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "sessions")
data class Session(
    @PrimaryKey val id: String,
    val title: String,
    @ColumnInfo(name = "first_message_preview") val firstMessagePreview: String,
    @ColumnInfo(name = "turn_count") val turnCount: Int = 0,
    @ColumnInfo(name = "injected_op_count") val injectedOpCount: Int = 0,
    @ColumnInfo(name = "created_at") val createdAt: Long,
    @ColumnInfo(name = "updated_at") val updatedAt: Long,
    @ColumnInfo(name = "pinned_at") val pinnedAt: Long? = null,
)
