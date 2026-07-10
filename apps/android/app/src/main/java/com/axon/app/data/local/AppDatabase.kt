package com.axon.app.data.local

import android.content.Context
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase
import com.axon.app.BuildConfig

@Database(entities = [AskHistoryEntry::class, Session::class], version = 3, exportSchema = true)
abstract class AppDatabase : RoomDatabase() {
    abstract fun askHistoryDao(): AskHistoryDao
    abstract fun sessionDao(): SessionDao

    companion object {
        val MIGRATION_1_2 = object : Migration(1, 2) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL(
                    """CREATE TABLE IF NOT EXISTS sessions (
                        id TEXT NOT NULL PRIMARY KEY,
                        title TEXT NOT NULL,
                        first_message_preview TEXT NOT NULL,
                        turn_count INTEGER NOT NULL DEFAULT 0,
                        injected_op_count INTEGER NOT NULL DEFAULT 0,
                        created_at INTEGER NOT NULL,
                        updated_at INTEGER NOT NULL,
                        pinned_at INTEGER
                    )"""
                )
                db.execSQL("CREATE INDEX IF NOT EXISTS index_sessions_pinned_updated ON sessions(pinned_at, updated_at)")
            }
        }

        // Mobile Session Model fields (android-contract.md) — client-side cache
        // columns only; see Session.kt kdoc for the joint-deferred server note.
        val MIGRATION_2_3 = object : Migration(2, 3) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL("ALTER TABLE sessions ADD COLUMN status TEXT NOT NULL DEFAULT 'active'")
                db.execSQL("ALTER TABLE sessions ADD COLUMN source_refs TEXT NOT NULL DEFAULT '[]'")
                db.execSQL("ALTER TABLE sessions ADD COLUMN draft TEXT")
                db.execSQL("ALTER TABLE sessions ADD COLUMN sync_version INTEGER")
            }
        }

        fun build(context: Context): AppDatabase =
            Room.databaseBuilder(context, AppDatabase::class.java, "axon.db")
                .addMigrations(MIGRATION_1_2, MIGRATION_2_3)
                .apply { if (BuildConfig.DEBUG) fallbackToDestructiveMigration() }
                .build()
    }
}
