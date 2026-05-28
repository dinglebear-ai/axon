package com.axon.app.data.repository

import com.axon.app.data.local.AskHistoryDao
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.remote.AxonClient
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * A hand-rolled in-memory fake for [AskHistoryDao].
 *
 * Room's DAO is an interface, so we can implement it directly without any test
 * framework dependency beyond the JDK.
 */
private class FakeAskHistoryDao : AskHistoryDao {
    val inserted = mutableListOf<AskHistoryEntry>()

    override fun recent(): Flow<List<AskHistoryEntry>> = flowOf(inserted.toList())

    override suspend fun insert(entry: AskHistoryEntry) {
        inserted.add(entry)
    }

    override suspend fun clearAll() {
        inserted.clear()
    }
}

class AxonRepositoryTest {

    private lateinit var server: MockWebServer
    private lateinit var client: AxonClient
    private lateinit var dao: FakeAskHistoryDao
    private lateinit var repo: AxonRepository

    @Before
    fun setUp() {
        server = MockWebServer()
        server.start()
        client = AxonClient(
            baseUrl = server.url("/").toString().trimEnd('/'),
            token = "test-token",
        )
        dao = FakeAskHistoryDao()
        repo = AxonRepository(
            client = client,
            askHistoryDao = dao,
            applicator = NoopModeOptionsApplicator,
        )
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    // ── withToken guard ───────────────────────────────────────────────────────

    /**
     * The `withToken` helper is private+inline. We exercise it by constructing
     * a repository whose client has no token and asserting every public suspend
     * function returns failure with the expected message.
     */
    @Test
    fun `ask returns failure with no-token message when client has no token`() = runBlocking {
        val emptyRepo = repoWithNoToken()
        val result = emptyRepo.ask("what is axon?")
        assertTrue(result.isFailure)
        val msg = result.exceptionOrNull()?.message.orEmpty()
        assertTrue(
            "expected no-token message, got: $msg",
            msg.contains("No API token configured"),
        )
    }

    @Test
    fun `query returns failure with no-token message when client has no token`() = runBlocking {
        val emptyRepo = repoWithNoToken()
        val result = emptyRepo.query("axon")
        assertTrue(result.isFailure)
        assertTrue(result.exceptionOrNull()?.message.orEmpty().contains("No API token configured"))
    }

    @Test
    fun `sources returns failure with no-token message when client has no token`() = runBlocking {
        val emptyRepo = repoWithNoToken()
        val result = emptyRepo.sources()
        assertTrue(result.isFailure)
        assertTrue(result.exceptionOrNull()?.message.orEmpty().contains("No API token configured"))
    }

    @Test
    fun `scrape returns failure with no-token message when client has no token`() = runBlocking {
        val emptyRepo = repoWithNoToken()
        val result = emptyRepo.scrape("https://example.com")
        assertTrue(result.isFailure)
        assertTrue(result.exceptionOrNull()?.message.orEmpty().contains("No API token configured"))
    }

    @Test
    fun `crawlSubmit returns failure with no-token message when client has no token`() = runBlocking {
        val emptyRepo = repoWithNoToken()
        val result = emptyRepo.crawlSubmit("https://example.com")
        assertTrue(result.isFailure)
        assertTrue(result.exceptionOrNull()?.message.orEmpty().contains("No API token configured"))
    }

    // ── ask happy path ────────────────────────────────────────────────────────

    @Test
    fun `ask maps network response to AskResultUi`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"query":"what is rust","answer":"a systems language","timing_ms":{"total_ms":123}}""")
                .addHeader("Content-Type", "application/json"),
        )
        val result = repo.ask("what is rust")
        assertTrue(result.isSuccess)
        val ui = result.getOrThrow()
        assertEquals("what is rust", ui.query)
        assertEquals("a systems language", ui.answer)
        assertEquals(123L, ui.timingMs)
    }

    @Test
    fun `ask maps response with missing timing_ms to null timingMs`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"query":"q","answer":"a"}""")
                .addHeader("Content-Type", "application/json"),
        )
        val result = repo.ask("q")
        assertTrue(result.isSuccess)
        val ui = result.getOrThrow()
        assertEquals(null, ui.timingMs)
    }

    @Test
    fun `ask propagates network failure as Result failure`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(500).setBody("oops"))
        val result = repo.ask("query")
        assertTrue(result.isFailure)
        val msg = result.exceptionOrNull()?.message.orEmpty()
        assertTrue(msg.contains("HTTP 500"))
    }

    // ── sources JSON parsing ──────────────────────────────────────────────────

    @Test
    fun `sources parses well-formed JsonArray tuples`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody(
                    """{"count":2,"limit":50,"offset":0,"urls":[["https://a.com",10],["https://b.com",5]]}""",
                )
                .addHeader("Content-Type", "application/json"),
        )
        val result = repo.sources()
        assertTrue(result.isSuccess)
        val list = result.getOrThrow()
        assertEquals(2, list.size)
        assertEquals("https://a.com", list[0].url)
        assertEquals(10, list[0].chunks)
        assertEquals("https://b.com", list[1].url)
        assertEquals(5, list[1].chunks)
    }

    @Test
    fun `sources silently skips malformed JsonArray entries`() = runBlocking {
        // One well-formed entry, one single-element (too short), one wrong type — only first survives.
        server.enqueue(
            MockResponse()
                .setBody(
                    """{"count":3,"limit":50,"offset":0,"urls":[["https://good.com",7],["https://short.com"],["https://bad.com","not-an-int"]]}""",
                )
                .addHeader("Content-Type", "application/json"),
        )
        val result = repo.sources()
        assertTrue(result.isSuccess)
        val list = result.getOrThrow()
        // The malformed entries are skipped via mapNotNull { runCatching ... }
        assertEquals(1, list.size)
        assertEquals("https://good.com", list[0].url)
    }

    @Test
    fun `sources returns empty list when urls array is empty`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"count":0,"limit":50,"offset":0,"urls":[]}""")
                .addHeader("Content-Type", "application/json"),
        )
        val result = repo.sources()
        assertTrue(result.isSuccess)
        assertTrue(result.getOrThrow().isEmpty())
    }

    // ── crawlStatus ───────────────────────────────────────────────────────────

    @Test
    fun `crawlStatus returns 'unknown' when status field is blank`() = runBlocking {
        // Real server shape: {"job": {...}} envelope with inner id/status/url/error_text fields.
        server.enqueue(
            MockResponse()
                .setBody("""{"job":{"id":"abc","status":"","url":"https://example.com","error_text":null}}""")
                .addHeader("Content-Type", "application/json"),
        )
        val result = repo.crawlStatus("abc")
        assertTrue(result.isSuccess)
        assertEquals("unknown", result.getOrThrow().status)
    }

    @Test
    fun `crawlStatus returns status string when non-blank`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"job":{"id":"abc","status":"running","url":"https://example.com","error_text":null}}""")
                .addHeader("Content-Type", "application/json"),
        )
        val result = repo.crawlStatus("abc")
        assertTrue(result.isSuccess)
        assertEquals("running", result.getOrThrow().status)
    }

    @Test
    fun `crawlStatus returns completed with pagesCrawled from result_json`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody(
                    """{"job":{"id":"abc","status":"completed","url":"https://example.com","error_text":null,"result_json":{"pages_crawled":5}}}""",
                )
                .addHeader("Content-Type", "application/json"),
        )
        val result = repo.crawlStatus("abc")
        assertTrue(result.isSuccess)
        val ui = result.getOrThrow()
        assertEquals("completed", ui.status)
        assertEquals(5, ui.pagesCrawled)
    }

    @Test
    fun `crawlStatus falls back to supplied jobId when id field is absent`() = runBlocking {
        // id defaults to "" when missing → ifBlank uses the passed-in jobId parameter.
        server.enqueue(
            MockResponse()
                .setBody("""{"job":{"id":"","status":"pending","url":"","error_text":null}}""")
                .addHeader("Content-Type", "application/json"),
        )
        val result = repo.crawlStatus("fallback-id")
        assertTrue(result.isSuccess)
        assertEquals("fallback-id", result.getOrThrow().jobId)
    }

    // ── ping delegates to healthz ─────────────────────────────────────────────

    @Test
    fun `ping returns true when healthz succeeds`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(200).setBody("ok"))
        assertTrue(repo.ping())
    }

    @Test
    fun `ping returns false when server is unreachable`() = runBlocking {
        server.shutdown()
        assertFalse(repo.ping())
    }

    // ── Ask history ───────────────────────────────────────────────────────────

    @Test
    fun `recordAskHistory delegates to dao insert`() = runBlocking {
        val entry = AskHistoryEntry(query = "q", answer = "a")
        repo.recordAskHistory(entry)
        assertEquals(1, dao.inserted.size)
        assertEquals("q", dao.inserted[0].query)
        assertEquals("a", dao.inserted[0].answer)
    }

    // ── retrieve ──────────────────────────────────────────────────────────────

    @Test
    fun `retrieve maps response body to RetrieveResultUi`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody(
                    """{"chunk_count":3,"content":"hello world","requested_url":"https://a.com","matched_url":"https://a.com/x","truncated":false,"warnings":[]}""",
                )
                .addHeader("Content-Type", "application/json"),
        )
        val result = repo.retrieve("https://a.com")
        assertTrue(result.isSuccess)
        val ui = result.getOrThrow()
        assertEquals("https://a.com", ui.requestedUrl)
        assertEquals("https://a.com/x", ui.matchedUrl)
        assertEquals(3, ui.chunkCount)
        assertEquals("hello world", ui.content)
        assertFalse(ui.truncated)
        assertTrue(ui.warnings.isEmpty())
    }

    @Test
    fun `retrieve falls back to caller url when response omits requested_url`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"chunk_count":0,"content":""}""")
                .addHeader("Content-Type", "application/json"),
        )
        val result = repo.retrieve("https://b.com")
        assertTrue(result.isSuccess)
        assertEquals("https://b.com", result.getOrThrow().requestedUrl)
    }

    @Test
    fun `retrieve forwards default token_budget to the server`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"chunk_count":0,"content":""}""")
                .addHeader("Content-Type", "application/json"),
        )
        repo.retrieve("https://c.com")
        val request = server.takeRequest()
        // Default DEFAULT_RETRIEVE_TOKEN_BUDGET = 64_000 — verify it lands on the wire.
        val body = request.body.readUtf8()
        assertTrue(
            "expected token_budget=64000 in body, got: $body",
            body.contains("\"token_budget\":64000") || body.contains("\"token_budget\": 64000"),
        )
    }

    @Test
    fun `retrieve forwards explicit token_budget override to the server`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"chunk_count":0,"content":""}""")
                .addHeader("Content-Type", "application/json"),
        )
        repo.retrieve("https://d.com", tokenBudget = 8_000)
        val request = server.takeRequest()
        assertTrue(request.body.readUtf8().contains("\"token_budget\":8000"))
    }

    @Test
    fun `retrieve propagates truncated and warnings to UI model`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody(
                    """{"chunk_count":2,"content":"abc","truncated":true,"warnings":["chunk-parse-failure","stale-cache"]}""",
                )
                .addHeader("Content-Type", "application/json"),
        )
        val result = repo.retrieve("https://e.com")
        assertTrue(result.isSuccess)
        val ui = result.getOrThrow()
        assertTrue(ui.truncated)
        assertEquals(listOf("chunk-parse-failure", "stale-cache"), ui.warnings)
    }

    @Test
    fun `retrieve returns failure with no-token message when client has no token`() = runBlocking {
        val emptyRepo = repoWithNoToken()
        val result = emptyRepo.retrieve("https://x.com")
        assertTrue(result.isFailure)
        assertTrue(result.exceptionOrNull()?.message.orEmpty().contains("No API token configured"))
    }

    // ── Helper ────────────────────────────────────────────────────────────────

    private fun repoWithNoToken(): AxonRepository {
        val noTokenClient = AxonClient(
            baseUrl = server.url("/").toString().trimEnd('/'),
            token = "",
        )
        return AxonRepository(
            client = noTokenClient,
            askHistoryDao = dao,
            applicator = NoopModeOptionsApplicator,
        )
    }
}
