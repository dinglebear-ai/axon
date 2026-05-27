package com.axon.app.data.remote

import com.axon.app.data.remote.models.IngestRequest
import com.axon.app.data.remote.models.SearchWebRequest
import com.axon.app.data.remote.models.SummarizeRequest
import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class AxonClientPhase2Test {
    private lateinit var server: MockWebServer
    private lateinit var client: AxonClient

    @Before fun setUp() {
        server = MockWebServer().also { it.start() }
        client = AxonClient(server.url("/").toString().trimEnd('/'), "test-token")
    }
    @After fun tearDown() { server.shutdown() }

    @Test fun `summarize posts to v1 summarize`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"urls":["https://a"],"summary":"hi","context_chars":10,"context_truncated":false}""").addHeader("Content-Type","application/json"))
        val r = client.summarize(SummarizeRequest(url = "https://a"))
        assertTrue(r.isSuccess)
        val req = server.takeRequest()
        assertEquals("POST", req.method)
        assertEquals("/v1/summarize", req.path)
        assertTrue(req.body.readUtf8().contains("\"url\":\"https://a\""))
    }

    @Test fun `searchWeb posts to v1 search and decodes hits + crawl jobs`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"query":"k","results":[{"title":"t","url":"https://x"}],"crawl_jobs":[{"job_id":"j1","url":"https://x"}]}""").addHeader("Content-Type","application/json"))
        val r = client.searchWeb(SearchWebRequest(query = "k"))
        assertTrue(r.isSuccess)
        val resp = r.getOrThrow()
        assertEquals(1, resp.results.size)
        assertEquals("j1", resp.crawlJobs[0].jobId)
    }

    @Test fun `ingestStart posts to v1 ingest and decodes AcceptedJob`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(202).setBody("""{"job_id":"abc","status":"pending"}""").addHeader("Content-Type","application/json"))
        val r = client.ingestStart(IngestRequest(sourceType = "github", target = "https://github.com/o/r"))
        assertTrue(r.isSuccess)
        assertEquals("abc", r.getOrThrow().jobId)
        val body = server.takeRequest().body.readUtf8()
        assertTrue(body.contains("\"source_type\":\"github\""))
    }

    @Test fun `ingestList GETs v1 ingest list and decodes ServiceJob array`() = runBlocking {
        server.enqueue(MockResponse().setBody("""[{"id":"j","status":"completed","source_type":"github","target":"https://github.com/o/r"}]""").addHeader("Content-Type","application/json"))
        val r = client.listJobs(AxonClient.JobKind.Ingest)
        assertTrue(r.isSuccess)
        assertEquals("j", r.getOrThrow()[0].id)
    }

    @Test fun `cancelJob POSTs v1 kind id cancel`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"canceled":true}""").addHeader("Content-Type","application/json"))
        val r = client.cancelJob(AxonClient.JobKind.Crawl, "j1")
        assertTrue(r.isSuccess && r.getOrThrow().canceled)
        assertEquals("/v1/crawl/j1/cancel", server.takeRequest().path)
    }

    @Test fun `status GETs v1 status`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"payload":{"pending":2}}""").addHeader("Content-Type","application/json"))
        val r = client.status()
        assertTrue(r.isSuccess)
    }

    @Test fun `doctor GETs v1 doctor`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"payload":{"qdrant":"ok"}}""").addHeader("Content-Type","application/json"))
        assertTrue(client.doctor().isSuccess)
    }

    @Test fun `suggest POSTs v1 suggest`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"urls":[{"url":"https://x","reason":"r"}]}""").addHeader("Content-Type","application/json"))
        assertTrue(client.suggest(focus = "rust").isSuccess)
    }

    @Test fun `domains GETs v1 domains`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"domains":[{"domain":"d","vectors":5}]}""").addHeader("Content-Type","application/json"))
        assertTrue(client.domains().isSuccess)
    }
}
