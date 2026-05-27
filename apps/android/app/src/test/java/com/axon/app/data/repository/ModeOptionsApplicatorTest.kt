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
import com.axon.app.ui.options.forms.AskFormKeys
import com.axon.app.ui.options.forms.CrawlFormKeys
import com.axon.app.ui.options.forms.IngestFormKeys
import com.axon.app.ui.options.forms.MapFormKeys
import com.axon.app.ui.options.forms.QueryFormKeys
import com.axon.app.ui.options.forms.ResearchFormKeys
import com.axon.app.ui.options.forms.ScrapeFormKeys
import com.axon.app.ui.options.forms.SearchWebFormKeys
import com.axon.app.ui.options.forms.SummarizeFormKeys
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

    @Test fun `Crawl override sets max pages and headers`() = runBlocking {
        ctx.modeOptionsDataStore.edit {
            it[CrawlFormKeys.MAX_PAGES] = 200
            it[CrawlFormKeys.INCLUDE_SUBDOMAINS] = true
            it[CrawlFormKeys.HEADERS] = setOf("Authorization: Bearer abc", "X-Trace: y")
        }
        val out = repo.apply(CrawlRequest(urls = listOf("https://a")))
        assertEquals(200, out.maxPages)
        assertEquals(true, out.includeSubdomains)
        assertEquals(2, out.headers.size)
        assertTrue("Authorization: Bearer abc" in out.headers)
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

    @Test fun `Ingest override sets include_source and collection`() = runBlocking {
        ctx.modeOptionsDataStore.edit {
            it[IngestFormKeys.INCLUDE_SOURCE] = false
            it[IngestFormKeys.COLLECTION] = "alt"
        }
        val out = repo.apply(IngestRequest(sourceType = "github", target = "o/r"))
        assertEquals(false, out.includeSource)
        assertEquals("alt", out.collection)
    }

    @Test fun `apply is a no-op when no overrides are set`() = runBlocking {
        val ask = AskRequest(query = "q")
        val out = repo.apply(ask)
        assertEquals(ask, out)
    }
}
