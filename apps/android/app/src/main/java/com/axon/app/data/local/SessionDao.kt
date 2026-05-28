package com.axon.app.data.local

import androidx.room.Dao
import androidx.room.Delete
import androidx.room.Query
import androidx.room.Upsert
import kotlinx.coroutines.flow.Flow

@Dao
interface SessionDao {
    @Query("SELECT * FROM sessions ORDER BY CASE WHEN pinned_at IS NULL THEN 1 ELSE 0 END, updated_at DESC")
    fun allSessions(): Flow<List<Session>>

    @Query("SELECT * FROM sessions WHERE id = :id")
    suspend fun getById(id: String): Session?

    @Upsert
    suspend fun upsert(session: Session)

    @Delete
    suspend fun delete(session: Session)

    @Query("UPDATE sessions SET pinned_at = :ts WHERE id = :id")
    suspend fun pin(id: String, ts: Long)

    @Query("UPDATE sessions SET pinned_at = NULL WHERE id = :id")
    suspend fun unpin(id: String)
}
