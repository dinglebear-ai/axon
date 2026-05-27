package com.axon.app.data.local

import android.content.Context
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase

// exportSchema = true: Room writes schema JSON to the schemas/ directory (configured via
// ksp { arg("room.schemaLocation", ...) } in build.gradle.kts), enabling migration verification.
// fallbackToDestructiveMigration is intentional — ask history is fully re-generable from the server.
@Database(entities = [AskHistoryEntry::class], version = 1, exportSchema = true)
abstract class AppDatabase : RoomDatabase() {
    abstract fun askHistoryDao(): AskHistoryDao

    companion object {
        fun build(context: Context): AppDatabase =
            Room.databaseBuilder(context, AppDatabase::class.java, "axon.db")
                .fallbackToDestructiveMigration()
                .build()
    }
}
