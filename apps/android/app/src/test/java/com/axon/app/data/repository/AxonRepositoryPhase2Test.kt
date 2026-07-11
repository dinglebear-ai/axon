package com.axon.app.data.repository

import com.axon.app.data.local.AskHistoryDao
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.core.api.AxonClient
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

private class NoopDao : AskHistoryDao {
    override fun recent(): Flow<List<AskHistoryEntry>> = flowOf(emptyList())
    override suspend fun insert(entry: AskHistoryEntry) {}
    override suspend fun clearAll() {}
}

class AxonRepositoryPhase2Test {
    private lateinit var server: MockWebServer
    private lateinit var repo: AxonRepository

    @Before fun setUp() {
        server = MockWebServer().also { it.start() }
        repo = AxonRepository(
            AxonClient(server.url("/").toString().trimEnd('/'), "t"),
            NoopDao(),
            NoopModeOptionsApplicator,
        )
    }
    @After fun tearDown() { server.shutdown() }

    @Test fun `summarize maps wire to UI`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"urls":["a"],"summary":"hi","context_chars":7,"context_truncated":false}""").addHeader("Content-Type","application/json"))
        val r = repo.summarize(listOf("a"))
        assertTrue(r.isSuccess)
        assertEquals("hi", r.getOrThrow().summary)
    }

    @Test fun `searchWeb maps results and crawl jobs`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"query":"k","results":[{"title":"t","url":"https://x"}],"crawl_jobs":[{"job_id":"j","url":"https://x"}]}""").addHeader("Content-Type","application/json"))
        val r = repo.searchWeb("k").getOrThrow()
        assertEquals(1, r.results.size)
        assertEquals("j", r.crawlJobs[0].jobId)
    }

    @Test fun `ingestStart returns jobId`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(202).setBody("""{"job_id":"abc","status":"pending"}""").addHeader("Content-Type","application/json"))
        assertEquals("abc", repo.ingestStart("github", "https://github.com/o/r").getOrThrow())
    }

    @Test fun `extractStart returns jobId`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(202).setBody("""{"job_id":"ex","status":"pending"}""").addHeader("Content-Type","application/json"))
        assertEquals("ex", repo.extractStart("https://example.com", "extract title").getOrThrow())
        val body = server.takeRequest().body.readUtf8()
        assertTrue(body.contains("\"urls\":[\"https://example.com\"]"))
        assertTrue(body.contains("\"prompt\":\"extract title\""))
    }

    @Test fun `listJobs returns full server array unchanged (no client-side slicing)`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"jobs":[{"id":"a","status":"x"},{"id":"b","status":"y"}],"limit":100,"offset":0}""").addHeader("Content-Type","application/json"))
        val jobs = repo.listJobs(JobFamily.Crawl).getOrThrow()
        assertEquals(2, jobs.size)
    }

    @Test fun `listJobs maps progressJson to JobUi`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody(
                    """{"jobs":[{"id":"j","status":"running","progress_json":{"lifecycle_progress":0.42,"pages_crawled":42}}],"limit":25,"offset":0}"""
                )
                .addHeader("Content-Type", "application/json")
        )

        val job = repo.listJobs(JobFamily.Crawl).getOrThrow().single()

        val progress = job.progressJson!!.jsonObject
        assertEquals("0.42", progress["lifecycle_progress"]!!.jsonPrimitive.content)
        assertEquals("42", progress["pages_crawled"]!!.jsonPrimitive.content)
    }

    @Test fun `listWatches maps watch definitions`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"watches":[{"id":"w","name":"Docs","task_type":"watch","enabled":true,"every_seconds":300}]}""").addHeader("Content-Type","application/json"))
        val watches = repo.listWatches().getOrThrow()
        assertEquals(1, watches.size)
        assertEquals("Docs", watches[0].name)
        assertEquals(true, watches[0].enabled)
    }

    @Test fun `summarize blocked by missing token`() = runBlocking {
        val r2 = AxonRepository(
            AxonClient(server.url("/").toString().trimEnd('/'), ""),
            NoopDao(),
            NoopModeOptionsApplicator,
        ).summarize(listOf("a"))
        assertTrue(r2.isFailure)
    }
}
