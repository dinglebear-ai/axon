package com.axon.app.core.api

import com.axon.app.core.api.models.ExtractRequest
import com.axon.app.core.api.models.SourceRequest
import com.axon.app.core.api.models.SearchWebRequest
import com.axon.app.core.api.models.SummarizeRequest
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

    @Test fun `searchWeb posts to v1 search and decodes hits plus source jobs`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"query":"k","results":[{"title":"t","url":"https://x"}],"source_jobs":[{"job_id":"j1","url":"https://x"}]}""").addHeader("Content-Type","application/json"))
        val r = client.searchWeb(SearchWebRequest(query = "k"))
        assertTrue(r.isSuccess)
        val resp = r.getOrThrow()
        assertEquals(1, resp.results.size)
        assertEquals("j1", resp.sourceJobs[0].jobId)
    }

    @Test fun `source submission posts to v1 sources and decodes AcceptedJob`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(202).setBody("""{"job_id":"abc","canonical_uri":"https://github.com/o/r","status":"queued"}""").addHeader("Content-Type","application/json"))
        val r = client.sourceSubmit(SourceRequest(source = "https://github.com/o/r"))
        assertTrue(r.isSuccess)
        assertEquals("abc", r.getOrThrow().jobId)
        val req = server.takeRequest()
        assertEquals("/v1/sources", req.path)
        val body = req.body.readUtf8()
        assertTrue(body.contains("\"source\":\"https://github.com/o/r\""))
        // `source_type` was a routing hint for the removed family-specific endpoint;
        // the unified pipeline classifies the target itself, so it must not be sent.
        assertTrue(!body.contains("source_type"))
        assertTrue(!body.contains("\"collection\""))
    }

    @Test fun `extractStart posts urls to v1 extract and decodes AcceptedJob`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(202).setBody("""{"job_id":"ex1","status":"pending"}""").addHeader("Content-Type","application/json"))
        val r = client.extractStart(
            ExtractRequest(
                urls = listOf("https://example.com"),
                prompt = "extract title",
                headers = listOf("Authorization: Bearer test"),
            )
        )
        assertTrue(r.isSuccess)
        assertEquals("ex1", r.getOrThrow().jobId)
        val req = server.takeRequest()
        assertEquals("POST", req.method)
        assertEquals("/v1/extract", req.path)
        val body = req.body.readUtf8()
        assertTrue(body.contains("\"urls\":[\"https://example.com\"]"))
        assertTrue(body.contains("\"prompt\":\"extract title\""))
        assertTrue(body.contains("\"headers\":[\"Authorization: Bearer test\"]"))
    }

    @Test fun `getJob decodes unified job summary`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"job_id":"j","kind":"source","status":"completed"}""").addHeader("Content-Type","application/json"))
        val r = client.getJob(AxonClient.JobKind.Extract, "j")
        assertTrue(r.isSuccess)
        assertEquals("j", r.getOrThrow().id)
        assertEquals("/v1/jobs/j", server.takeRequest().path)
    }

    @Test fun `listJobs GETs unified jobs and decodes summaries`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"items":[{"job_id":"j","kind":"source","status":"completed"}],"limit":25}""").addHeader("Content-Type","application/json"))
        val r = client.listJobs(AxonClient.JobKind.Source)
        assertTrue(r.isSuccess)
        assertEquals("j", r.getOrThrow()[0].id)
    }

    @Test fun `listJobs decodes progress json`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody(
                    """{"items":[{"job_id":"j","kind":"source","status":"running","counts":{"lifecycle_progress":0.42,"pages_crawled":42}}],"limit":25}"""
                )
                .addHeader("Content-Type", "application/json")
        )

        val r = client.listJobs(AxonClient.JobKind.Source)

        assertTrue(r.isSuccess)
        val progress = r.getOrThrow()[0].progressJson!!.jsonObject
        assertEquals("0.42", progress["lifecycle_progress"]!!.jsonPrimitive.content)
        assertEquals("42", progress["pages_crawled"]!!.jsonPrimitive.content)
    }

    @Test fun `listWatches GETs v1 watches and decodes watch page`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"items":[{"watch_id":"w1","source_id":"Docs","enabled":true,"schedule":{"every_seconds":300},"next_run_at":"2026-06-04T12:00:00Z"}]}""").addHeader("Content-Type","application/json"))
        val r = client.listWatches()
        assertTrue(r.isSuccess)
        assertEquals("Docs", r.getOrThrow()[0].displayName)
        val req = server.takeRequest()
        assertEquals("GET", req.method)
        assertTrue("expected path starting with /v1/watches, got ${req.path}", req.path!!.startsWith("/v1/watches"))
    }

    @Test fun `cancelJob POSTs unified job cancel route`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"job_id":"j1","status":"cancelled"}""").addHeader("Content-Type","application/json"))
        val r = client.cancelJob(AxonClient.JobKind.Source, "j1")
        assertTrue(r.isSuccess && r.getOrThrow().canceled)
        assertEquals("/v1/jobs/j1/cancel", server.takeRequest().path)
    }

    @Test fun `status GETs v1 status`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"payload":{"pending":2}}""").addHeader("Content-Type","application/json"))
        val r = client.status()
        assertTrue(r.isSuccess)
    }

    @Test fun `doctor GETs v1 doctor`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"payload":{"qdrant":"ok"}}""").addHeader("Content-Type","application/json"))
        assertTrue(client.doctor().isSuccess)
        val req = server.takeRequest()
        assertEquals("GET", req.method)
        assertEquals("/v1/doctor", req.path)
    }

    @Test fun `suggest POSTs v1 suggest`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"urls":[{"url":"https://x","reason":"r"}]}""").addHeader("Content-Type","application/json"))
        assertTrue(client.suggest(focus = "rust").isSuccess)
        val req = server.takeRequest()
        assertEquals("POST", req.method)
        assertEquals("/v1/suggest", req.path)
    }

    @Test fun `suggest decodes production suggestions field`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"suggestions":[{"url":"https://x","reason":"r"}]}""").addHeader("Content-Type","application/json"))

        val response = client.suggest().getOrThrow()

        assertEquals("https://x", response.suggestions.single().url)
    }

    @Test fun `domains GETs v1 domains`() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"domains":[{"domain":"d","vectors":5}]}""").addHeader("Content-Type","application/json"))
        assertTrue(client.domains().isSuccess)
        val req = server.takeRequest()
        assertEquals("GET", req.method)
        assertTrue("expected path starting with /v1/domains, got ${req.path}", req.path!!.startsWith("/v1/domains"))
    }
}
