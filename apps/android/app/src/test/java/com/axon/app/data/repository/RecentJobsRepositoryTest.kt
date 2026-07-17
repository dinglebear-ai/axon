package com.axon.app.data.repository

import android.content.Context
import androidx.datastore.preferences.core.edit
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Robolectric-backed integration tests for [RecentJobsRepository].
 *
 * Each test starts with a clean DataStore by clearing the preferences in `tearDown`.
 * Tests run on Robolectric SDK 33 — chosen to match the minSdk-aware Compose tooling
 * used elsewhere in the suite while staying inside the supported Robolectric range.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [33])
class RecentJobsRepositoryTest {
    private val ctx: Context = ApplicationProvider.getApplicationContext()
    private val repo = RecentJobsRepository(ctx)

    @After fun tearDown() = runBlocking {
        ctx.recentJobsDataStore.edit { it.clear() }
        Unit
    }

    @Test fun `add then read returns the persisted entry`() = runBlocking {
        repo.add(RecentJob("j1", "source", "github.com/o/r", 100L))
        val items = repo.recent.first()
        assertEquals(1, items.size)
        assertEquals("j1", items[0].jobId)
        assertEquals("source", items[0].kind)
    }

    @Test fun `add dedupes by jobId — re-submitting same job replaces in place`() = runBlocking {
        repo.add(RecentJob("j1", "source", "t1", 100L))
        repo.add(RecentJob("j1", "source", "t1-renamed", 200L))
        val items = repo.recent.first()
        assertEquals("expected single entry after dedup", 1, items.size)
        // The newer submittedAt + target win because the entry is replaced.
        assertEquals("j1", items[0].jobId)
        assertEquals(200L, items[0].submittedAt)
        assertEquals("t1-renamed", items[0].target)
    }

    @Test fun `add enforces LRU cap at MAX_RECENT_JOBS`() = runBlocking {
        // Insert 105 distinct jobs; only the 100 most-recently-added survive.
        // Newest entries have the highest submittedAt — they should win.
        repeat(105) { i ->
            repo.add(RecentJob(jobId = "j$i", kind = "source", target = "t$i", submittedAt = i.toLong()))
        }
        val items = repo.recent.first()
        assertEquals(100, items.size)
        // Sorted descending by submittedAt → j104 is first, j5 is last (j0..j4 evicted).
        assertEquals("j104", items.first().jobId)
        assertTrue("expected j0..j4 evicted by LRU cap", items.none { it.jobId in setOf("j0","j1","j2","j3","j4") })
    }

    @Test fun `forget removes the entry by jobId`() = runBlocking {
        repo.add(RecentJob("j1", "source", "t1", 100L))
        repo.add(RecentJob("j2", "source", "t2", 200L))
        repo.forget("j1")
        val items = repo.recent.first()
        assertEquals(listOf("j2"), items.map { it.jobId })
    }
}
