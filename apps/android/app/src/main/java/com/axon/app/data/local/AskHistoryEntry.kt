package com.axon.app.data.local

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "ask_history")
data class AskHistoryEntry(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val query: String,
    val answer: String,
    @ColumnInfo(name = "asked_at") val askedAt: Long = System.currentTimeMillis(),
)
