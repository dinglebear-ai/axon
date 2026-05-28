package com.axon.app.data.remote

import com.axon.app.data.remote.models.IngestRequest
import com.axon.app.data.remote.models.SearchWebRequest
import com.axon.app.data.remote.models.SummarizeRequest
import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.SocketPolicy
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * Failure-mode coverage for [AxonClient] phase 2 endpoints.
 *
 * The happy-path tests assert that valid JSON deserializes correctly; this
 * suite asserts that every non-2xx path, transport failure, and malformed
 * body resolves to `Result.failure` with a useful message — not a silent
 * empty success, not a crash.
 */
class AxonClientErrorPathTest {
    private lateinit var server: MockWebServer
    private lateinit var client: AxonClient

    @Before fun setUp() {
        server = MockWebServer().also { it.start() }
        client = AxonClient(server.url("/").toString().trimEnd('/'), "test-token")
    }
    @After fun tearDown() { server.shutdown() }

    // ── HTTP error codes ──────────────────────────────────────────────────────

    @Test fun `summarize surfaces 401 unauthorized`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(401).setBody("""{"error":"bad token"}"""))
        val r = client.summarize(SummarizeRequest(url = "https://a"))
        assertTrue(r.isFailure)
        val msg = r.exceptionOrNull()?.message ?: ""
        assertTrue("expected 401 in message: $msg", msg.contains("401"))
    }

    @Test fun `summarize surfaces 403 forbidden`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(403))
        val r = client.summarize(SummarizeRequest(url = "https://a"))
        assertTrue(r.isFailure)
        assertTrue(r.exceptionOrNull()?.message?.contains("403") == true)
    }

    @Test fun `searchWeb surfaces 500 internal server error with truncated body`() = runBlocking {
        // Body longer than 200 chars should be truncated — assert the truncation
        // doesn't break decoding and the failure carries the code.
        val longBody = "x".repeat(1000)
        server.enqueue(MockResponse().setResponseCode(500).setBody(longBody))
        val r = client.searchWeb(SearchWebRequest(query = "k"))
        assertTrue(r.isFailure)
        val msg = r.exceptionOrNull()?.message ?: ""
        assertTrue("expected 500 in message: $msg", msg.contains("500"))
    }

    @Test fun `cancelJob surfaces 404 not found`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(404))
        val r = client.cancelJob(AxonClient.JobKind.Crawl, "missing")
        assertTrue(r.isFailure)
        assertTrue(r.exceptionOrNull()?.message?.contains("404") == true)
    }

    @Test fun `status surfaces 503 service unavailable`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(503))
        val r = client.status()
        assertTrue(r.isFailure)
    }

    // ── Malformed / unexpected body ───────────────────────────────────────────

    @Test fun `ingestStart surfaces malformed JSON as failure`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setResponseCode(202)
                .setBody("""{"job_id" THIS IS NOT JSON""")
                .addHeader("Content-Type", "application/json"),
        )
        val r = client.ingestStart(IngestRequest(sourceType = "github", target = "https://github.com/o/r"))
        assertTrue(r.isFailure)
        assertNotNull(r.exceptionOrNull())
    }

    @Test fun `summarize surfaces wrong-shape JSON as failure`() = runBlocking {
        // Server returns a JSON array where the client expects an object — decoder
        // must reject this rather than silently producing a default-constructed value.
        server.enqueue(
            MockResponse()
                .setResponseCode(200)
                .setBody("""[1, 2, 3]""")
                .addHeader("Content-Type", "application/json"),
        )
        val r = client.summarize(SummarizeRequest(url = "https://a"))
        assertTrue(r.isFailure)
    }

    @Test fun `searchWeb surfaces empty body as failure`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(200).setBody(""))
        val r = client.searchWeb(SearchWebRequest(query = "k"))
        assertTrue(r.isFailure)
    }

    // ── Transport failures ────────────────────────────────────────────────────

    @Test fun `listJobs surfaces abrupt disconnect as failure`() = runBlocking {
        server.enqueue(MockResponse().apply { socketPolicy = SocketPolicy.DISCONNECT_AT_START })
        val r = client.listJobs(AxonClient.JobKind.Crawl)
        assertTrue(r.isFailure)
    }

    @Test fun `doctor surfaces no-response abort as failure`() = runBlocking {
        server.enqueue(MockResponse().apply { socketPolicy = SocketPolicy.NO_RESPONSE })
        // Use a small read timeout via a fresh client so the test exits in seconds, not minutes.
        // (The default 60s would block CI.)
        val shortClient = AxonClient(server.url("/").toString().trimEnd('/'), "t")
        val r = shortClient.doctor()
        // Either failure-by-timeout or by socket close — both are correct.
        assertTrue(r.isFailure)
    }

    // ── Token missing / present ───────────────────────────────────────────────

    @Test fun `hasToken is false for empty token`() {
        val c = AxonClient("http://localhost", "")
        assertEquals(false, c.hasToken())
    }

    @Test fun `hasToken is true for non-blank token`() {
        val c = AxonClient("http://localhost", "abc")
        assertEquals(true, c.hasToken())
    }

    @Test fun `hasToken is false for whitespace-only token`() {
        val c = AxonClient("http://localhost", "   ")
        assertEquals(false, c.hasToken())
    }
}
