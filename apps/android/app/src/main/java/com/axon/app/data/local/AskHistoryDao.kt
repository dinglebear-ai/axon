package com.axon.app.data.local

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import kotlinx.coroutines.flow.Flow

@Dao
interface AskHistoryDao {
    @Query("SELECT * FROM ask_history ORDER BY asked_at DESC LIMIT 50")
    fun recent(): Flow<List<AskHistoryEntry>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(entry: AskHistoryEntry)

    @Query("DELETE FROM ask_history")
    suspend fun clearAll()
}
