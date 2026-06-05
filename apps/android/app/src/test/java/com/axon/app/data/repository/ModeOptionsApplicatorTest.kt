package com.axon.app.data.repository

import android.content.Context
import androidx.datastore.preferences.core.edit
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.axon.app.data.remote.AskRequest
import com.axon.app.data.remote.CrawlRequest
import com.axon.app.data.remote.MapRequest
import com.axon.app.data.remote.QueryRequest
import com.axon.app.data.remote.ResearchRequest
import com.axon.app.data.remote.ScrapeRequest
import com.axon.app.data.remote.models.IngestRequest
import com.axon.app.data.remote.models.SearchWebRequest
import com.axon.app.data.remote.models.SummarizeRequest
import com.axon.app.data.repository.options.AskFormKeys
import com.axon.app.data.repository.options.CrawlFormKeys
import com.axon.app.data.repository.options.IngestFormKeys
import com.axon.app.data.repository.options.MapFormKeys
import com.axon.app.data.repository.options.QueryFormKeys
import com.axon.app.data.repository.options.ResearchFormKeys
import com.axon.app.data.repository.options.ScrapeFormKeys
import com.axon.app.data.repository.options.SearchWebFormKeys
import com.axon.app.data.repository.options.SummarizeFormKeys
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Per-DTO override tests for [ModeOptionsRepository] (the [ModeOptionsApplicator] impl).
 *
 * For each wire DTO type we set a single DataStore key, call apply(), and assert
 * the override landed in the merged request. Covers the R5 contract claim that
 * AxonRepository can stay ignorant of which fields exist per mode.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [33])
class ModeOptionsApplicatorTest {
    private val ctx: Context = ApplicationProvider.getApplicationContext()
    private val repo = ModeOptionsRepository(ctx)

    @After fun tearDown() = runBlocking {
        ctx.modeOptionsDataStore.edit { it.clear() }
        Unit
    }

    @Test fun `Ask override merges chunk_limit and diagnostics`() = runBlocking {
        ctx.modeOptionsDataStore.edit {
            it[AskFormKeys.CHUNK_LIMIT] = 42
            it[AskFormKeys.DIAGNOSTICS] = true
        }
        val out = repo.apply(AskRequest(query = "q"))
        assertEquals(42, out.chunkLimit)
        assertEquals(true, out.diagnostics)
    }

    @Test fun `Ask override does not stomp call-site value`() = runBlocking {
        ctx.modeOptionsDataStore.edit { it[AskFormKeys.CHUNK_LIMIT] = 10 }
        val out = repo.apply(AskRequest(query = "q", chunkLimit = 99))
        // Call-site value wins.
        assertEquals(99, out.chunkLimit)
    }

    @Test fun `Query override sets limit and collection`() = runBlocking {
        ctx.modeOptionsDataStore.edit {
            it[QueryFormKeys.LIMIT] = 25
            it[QueryFormKeys.COLLECTION] = "alt"
        }
        val out = repo.apply(QueryRequest(query = "q"))
        assertEquals(25, out.limit)
        assertEquals("alt", out.collection)
    }

    @Test fun `Summarize override sets renderMode and selectors`() = runBlocking {
        ctx.modeOptionsDataStore.edit {
            it[SummarizeFormKeys.RENDER_MODE] = "chrome"
            it[SummarizeFormKeys.ROOT_SELECTOR] = "main"
            it[SummarizeFormKeys.EXCLUDE_SELECTOR] = "footer"
        }
        val out = repo.apply(SummarizeRequest(urls = listOf("https://a")))
        assertEquals("chrome", out.renderMode)
        assertEquals("main", out.rootSelector)
        assertEquals("footer", out.excludeSelector)
    }

    @Test fun `Research override sets limit`() = runBlocking {
        ctx.modeOptionsDataStore.edit { it[ResearchFormKeys.LIMIT] = 7 }
        val out = repo.apply(ResearchRequest(query = "q"))
        assertEquals(7, out.limit)
    }

    @Test fun `Scrape override sets render mode + format + collection`() = runBlocking {
        ctx.modeOptionsDataStore.edit {
            it[ScrapeFormKeys.RENDER_MODE] = "chrome"
            it[ScrapeFormKeys.FORMAT] = "html"
            it[ScrapeFormKeys.COLLECTION] = "alt"
        }
        val out = repo.apply(ScrapeRequest(url = "https://a"))
        assertEquals("chrome", out.renderMode)
        assertEquals("html", out.format)
        assertEquals("alt", out.collection)
    }

    /**
     * EncryptedHeadersStore round-trips through EncryptedSharedPreferences, which
     * depends on the AndroidKeyStore HAL. Robolectric SDK 33 ships a working
     * shim, but assume-skip the keystore-backed tests on CI images where it isn't
     * — the persistence is the contract under test, not the keystore.
     */
    private fun encryptedHeadersAvailable(): Boolean {
        val probe = EncryptedHeadersStore(ctx)
        return try {
            probe.write("__probe__", listOf("X-Probe: 1"))
            EncryptedHeadersStore(ctx).read("__probe__") == listOf("X-Probe: 1")
        } catch (_: Throwable) {
            false
        } finally {
            runCatching { probe.clear("__probe__") }
        }
    }

    @Test fun `Crawl override sets max pages and headers`() = runBlocking {
        org.junit.Assume.assumeTrue(
            "Robolectric keystore unavailable — EncryptedSharedPreferences round-trip failed",
            encryptedHeadersAvailable(),
        )
        ctx.modeOptionsDataStore.edit {
            it[CrawlFormKeys.MAX_PAGES] = 200
            it[CrawlFormKeys.INCLUDE_SUBDOMAINS] = true
        }
        // Headers live in the EncryptedHeadersStore — write via the repo's
        // encrypted convenience helper. This exercises the same path the form uses.
        repo.writeEncryptedHeaders(
            EncryptedHeadersStore.KEY_CRAWL_HEADERS,
            listOf("Authorization: Bearer abc", "X-Trace: y"),
        )
        val out = repo.apply(CrawlRequest(urls = listOf("https://a")))
        assertEquals(200, out.maxPages)
        assertEquals(true, out.includeSubdomains)
        assertEquals(2, out.headers.size)
        assertTrue("Authorization: Bearer abc" in out.headers)
    }

    @Test fun `Crawl headers do not leak into the plaintext DataStore`() = runBlocking {
        org.junit.Assume.assumeTrue(
            "Robolectric keystore unavailable — EncryptedSharedPreferences round-trip failed",
            encryptedHeadersAvailable(),
        )
        // Regression guard for the PR-#142 critical fix: writing user-supplied
        // headers must NOT touch the mode_options DataStore.
        repo.writeEncryptedHeaders(
            EncryptedHeadersStore.KEY_CRAWL_HEADERS,
            listOf("Authorization: Bearer never-leak-this"),
        )
        val prefs = ctx.modeOptionsDataStore.data.first()
        // The legacy key name no longer exists in CrawlFormKeys.ALL; the DataStore
        // should be empty for any header-shaped value. We scan all entries to make
        // sure no key contains the secret payload — defends against a regression
        // where a new key accidentally persists header data.
        val leakedKey = prefs.asMap().entries.firstOrNull { (_, v) ->
            v.toString().contains("never-leak-this")
        }
        assertEquals(null, leakedKey)
    }

    @Test fun `Crawl call-site headers win over persisted encrypted headers`() = runBlocking {
        org.junit.Assume.assumeTrue(
            "Robolectric keystore unavailable — EncryptedSharedPreferences round-trip failed",
            encryptedHeadersAvailable(),
        )
        repo.writeEncryptedHeaders(
            EncryptedHeadersStore.KEY_CRAWL_HEADERS,
            listOf("X-Persisted: 1"),
        )
        val out = repo.apply(
            CrawlRequest(urls = listOf("https://a"), headers = listOf("X-Inline: 2")),
        )
        assertEquals(listOf("X-Inline: 2"), out.headers)
    }

    @Test fun `Map override sets limit and offset`() = runBlocking {
        ctx.modeOptionsDataStore.edit {
            it[MapFormKeys.LIMIT] = 50
            it[MapFormKeys.OFFSET] = 100
        }
        val out = repo.apply(MapRequest(url = "https://a"))
        assertEquals(50, out.limit)
        assertEquals(100, out.offset)
    }

    @Test fun `SearchWeb override sets time_range`() = runBlocking {
        ctx.modeOptionsDataStore.edit {
            it[SearchWebFormKeys.LIMIT] = 5
            it[SearchWebFormKeys.TIME_RANGE] = "week"
        }
        val out = repo.apply(SearchWebRequest(query = "q"))
        assertEquals(5, out.limit)
        assertEquals("week", out.timeRange)
    }

    @Test fun `Ingest override sets include_source only`() = runBlocking {
        ctx.modeOptionsDataStore.edit {
            it[IngestFormKeys.INCLUDE_SOURCE] = false
        }
        val out = repo.apply(IngestRequest(sourceType = "github", target = "o/r"))
        assertEquals(false, out.includeSource)
    }

    @Test fun `apply is a no-op when no overrides are set`() = runBlocking {
        val ask = AskRequest(query = "q")
        val out = repo.apply(ask)
        assertEquals(ask, out)
    }
}
