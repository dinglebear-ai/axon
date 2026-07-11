package com.axon.app.data.local

import android.content.Context
import androidx.room.Room
import androidx.sqlite.db.SupportSQLiteDatabase
import androidx.sqlite.db.SupportSQLiteOpenHelper
import androidx.sqlite.db.framework.FrameworkSQLiteOpenHelperFactory
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Covers two things for the Mobile Session Model fields
 * (`status`/`source_refs`/`draft`/`sync_version` -- android-contract.md):
 *
 * 1. [SessionDao] round-trips the new columns through an in-memory Room DB.
 * 2. [AppDatabase.MIGRATION_2_3] adds those columns with the documented
 *    defaults on top of a real (pre-migration) v2 `sessions` table, without
 *    disturbing existing rows.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [33])
class SessionDaoTest {
    private val ctx: Context = ApplicationProvider.getApplicationContext()

    private val v2SessionsCreateSql = """
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT NOT NULL PRIMARY KEY,
            title TEXT NOT NULL,
            first_message_preview TEXT NOT NULL,
            turn_count INTEGER NOT NULL DEFAULT 0,
            injected_op_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            pinned_at INTEGER
        )
    """.trimIndent()

    @Test fun `dao upsert and read round-trips the new mobile session fields`() = runBlocking {
        val db = Room.inMemoryDatabaseBuilder(ctx, AppDatabase::class.java)
            .allowMainThreadQueries()
            .build()
        val dao = db.sessionDao()

        val session = Session(
            id = "s1",
            title = "Title",
            firstMessagePreview = "preview",
            turnCount = 1,
            injectedOpCount = 0,
            createdAt = 10,
            updatedAt = 20,
            pinnedAt = null,
            status = "archived",
            sourceRefsJson = Session.encodeSourceRefs(listOf("job:1", "artifact:2")),
            draft = "draft in progress",
            syncVersion = 3,
        )
        dao.upsert(session)

        val loaded = dao.getById("s1")
        assertNotNull(loaded)
        assertEquals("archived", loaded!!.status)
        assertEquals(listOf("job:1", "artifact:2"), loaded.sourceRefs)
        assertEquals("draft in progress", loaded.draft)
        assertEquals(3L, loaded.syncVersion)

        db.close()
    }

    @Test fun `dao applies documented defaults when new fields are omitted`() = runBlocking {
        val db = Room.inMemoryDatabaseBuilder(ctx, AppDatabase::class.java)
            .allowMainThreadQueries()
            .build()
        val dao = db.sessionDao()

        dao.upsert(
            Session(
                id = "s2",
                title = "Title",
                firstMessagePreview = "preview",
                turnCount = 0,
                injectedOpCount = 0,
                createdAt = 1,
                updatedAt = 1,
            )
        )

        val loaded = dao.getById("s2")
        assertNotNull(loaded)
        assertEquals("active", loaded!!.status)
        assertTrue(loaded.sourceRefs.isEmpty())
        assertNull(loaded.draft)
        assertNull(loaded.syncVersion)

        db.close()
    }

    @Test fun `migration 2 to 3 adds new columns with defaults and preserves existing rows`() {
        val dbName = "migration-2-3-test-${System.nanoTime()}.db"
        val configuration = SupportSQLiteOpenHelper.Configuration.builder(ctx)
            .name(dbName)
            .callback(object : SupportSQLiteOpenHelper.Callback(2) {
                override fun onCreate(db: SupportSQLiteDatabase) {
                    db.execSQL(v2SessionsCreateSql)
                }

                override fun onUpgrade(db: SupportSQLiteDatabase, oldVersion: Int, newVersion: Int) {
                    // Not exercised: this test calls Migration.migrate() directly.
                }
            })
            .build()
        val helper = FrameworkSQLiteOpenHelperFactory().create(configuration)
        val db = helper.writableDatabase
        try {
            db.execSQL(
                """INSERT INTO sessions
                    (id, title, first_message_preview, turn_count, injected_op_count, created_at, updated_at, pinned_at)
                    VALUES ('s1', 'Pre-migration', 'preview', 2, 1, 10, 20, NULL)"""
            )

            AppDatabase.MIGRATION_2_3.migrate(db)

            db.query(
                "SELECT status, source_refs, draft, sync_version, title FROM sessions WHERE id = 's1'"
            ).use { cursor ->
                assertTrue(cursor.moveToFirst())
                assertEquals("active", cursor.getString(0))
                assertEquals("[]", cursor.getString(1))
                assertTrue(cursor.isNull(2))
                assertTrue(cursor.isNull(3))
                assertEquals("Pre-migration", cursor.getString(4))
            }
        } finally {
            db.close()
            ctx.deleteDatabase(dbName)
        }
    }
}
