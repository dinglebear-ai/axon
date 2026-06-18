package com.axon.app.data.remote

import com.axon.app.data.auth.AuthConfig
import com.axon.app.data.auth.OAuthTokenSource
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.flow.toList
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.util.concurrent.TimeUnit

class AxonClientTest {

    private lateinit var server: MockWebServer
    private lateinit var client: AxonClient

    private class FakeOAuthTokenSource(
        private val token: String = "oauth-access-token",
    ) : OAuthTokenSource {
        override suspend fun freshAccessToken(): Result<String> = Result.success(token)
        override fun isSignedIn(): Boolean = true
    }

    @Before
    fun setUp() {
        server = MockWebServer()
        server.start()
        client = AxonClient(
            baseUrl = server.url("/").toString().trimEnd('/'),
            token = "test-token",
        )
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    // ── Original tests ────────────────────────────────────────────────────────

    @Test
    fun `healthz returns true when server responds 200`() = runBlocking {
        server.enqueue(MockResponse().setBody("ok").setResponseCode(200))
        val result = client.healthz()
        assertTrue(result.isSuccess)
        val req = server.takeRequest()
        assertEquals("/healthz", req.path)
    }

    @Test
    fun `ask sends auth header and deserializes response`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"query":"hello","answer":"world","timing_ms":{"total_ms":500}}""")
                .addHeader("Content-Type", "application/json")
        )
        val result = client.ask(AskRequest(query = "hello"))
        assertTrue(result.isSuccess)
        assertEquals("world", result.getOrThrow().answer)
        val req = server.takeRequest()
        assertEquals("Bearer test-token", req.getHeader("Authorization"))
        assertEquals("/v1/ask", req.path)
    }

    @Test
    fun `chat sends message without RAG fields and deserializes response`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"message":"hello","answer":"plain answer","model":"chat-model"}""")
                .addHeader("Content-Type", "application/json")
        )

        val result = client.chat(ChatRequest(message = "hello"))

        assertTrue(result.isSuccess)
        assertEquals("plain answer", result.getOrThrow().answer)
        assertEquals("chat-model", result.getOrThrow().model)
        val req = server.takeRequest()
        assertEquals("Bearer test-token", req.getHeader("Authorization"))
        assertEquals("/v1/chat", req.path)
        assertEquals("""{"message":"hello"}""", req.body.readUtf8())
    }

    @Test
    fun `askStream reads nested done result answer from server SSE`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody(
                    """
                    data: {"type":"meta","phase":"retrieving"}
                    data: {"type":"delta","text":"streamed "}
                    data: {"type":"delta","text":"answer"}
                    data: {"type":"done","result":{"query":"hello","answer":"final answer","timing_ms":null}}

                    """.trimIndent(),
                )
                .addHeader("Content-Type", "text/event-stream"),
        )

        val events = client.askStream(AskRequest(query = "hello")).toList()

        assertEquals(4, events.size)
        assertEquals(AskStreamEvent.Meta("retrieving"), events[0])
        assertEquals(AskStreamEvent.Delta("streamed "), events[1])
        assertEquals(AskStreamEvent.Delta("answer"), events[2])
        assertEquals(AskStreamEvent.Done("final answer"), events[3])
        assertEquals("/v1/ask/stream", server.takeRequest().path)
    }

    @Test
    fun `chatStream reads direct chat done answer from server SSE`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody(
                    """
                    data: {"type":"meta","phase":"chatting"}
                    data: {"type":"delta","text":"plain "}
                    data: {"type":"done","answer":"plain answer"}

                    """.trimIndent(),
                )
                .addHeader("Content-Type", "text/event-stream"),
        )

        val events = client.chatStream(ChatRequest(message = "hello")).toList()

        assertEquals(3, events.size)
        assertEquals(AskStreamEvent.Meta("chatting"), events[0])
        assertEquals(AskStreamEvent.Delta("plain "), events[1])
        assertEquals(AskStreamEvent.Done("plain answer"), events[2])
        val req = server.takeRequest()
        assertEquals("/v1/chat/stream", req.path)
        assertEquals("""{"message":"hello"}""", req.body.readUtf8())
    }

    @Test
    fun `query deserializes results list`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"results":[{"rank":1,"score":0.9,"rerank_score":0.0,"url":"https://a.com","source":"a.com","snippet":"some text","chunk_index":null}]}""")
                .addHeader("Content-Type", "application/json")
        )
        val result = client.query(QueryRequest(query = "test"))
        assertTrue(result.isSuccess)
        assertEquals(1, result.getOrThrow().results.size)
        assertEquals("https://a.com", result.getOrThrow().results[0].url)
    }

    @Test
    fun `panelEnv requests panel env endpoint with configured auth headers`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"path":"~/.axon/.env","raw_env":"QDRANT_URL=http://qdrant","restart_required":false}""")
                .addHeader("Content-Type", "application/json"),
        )

        val result = client.panelEnv()

        assertTrue(result.isSuccess)
        assertEquals("QDRANT_URL=http://qdrant", result.getOrThrow().rawEnv)
        val req = server.takeRequest()
        assertEquals("/api/panel/env", req.path)
        assertEquals(null, req.getHeader("Authorization"))
        assertEquals(null, req.getHeader("x-api-key"))
        assertEquals("test-token", req.getHeader("x-axon-panel-token"))
    }

    @Test
    fun `savePanelEnv sends request with api bearer token`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"ok":true,"message":"saved","restart_required":true}""")
                .addHeader("Content-Type", "application/json"),
        )

        val result = client.savePanelEnv("GITHUB_TOKEN=secret\n")

        assertTrue(result.isSuccess)
        val req = server.takeRequest()
        assertEquals("/api/panel/env", req.path)
        assertEquals("PUT", req.method)
        assertEquals(null, req.getHeader("Authorization"))
        assertEquals(null, req.getHeader("x-api-key"))
        assertEquals("test-token", req.getHeader("x-axon-panel-token"))
    }

    // ── Non-2xx HTTP status ───────────────────────────────────────────────────

    @Test
    fun `ask returns failure with HTTP status code when server responds 500`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(500).setBody("Internal Server Error"))
        val result = client.ask(AskRequest(query = "hello"))
        assertTrue(result.isFailure)
        val msg = result.exceptionOrNull()?.message.orEmpty()
        assertTrue("failure message should contain 'HTTP 500'", msg.contains("HTTP 500"))
    }

    @Test
    fun `ask returns failure when server responds 401`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(401).setBody("Unauthorized"))
        val result = client.ask(AskRequest(query = "hello"))
        assertTrue(result.isFailure)
        val msg = result.exceptionOrNull()?.message.orEmpty()
        assertTrue("failure message should contain 'HTTP 401'", msg.contains("HTTP 401"))
    }

    @Test
    fun `query returns failure when server responds 404`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(404).setBody("Not Found"))
        val result = client.query(QueryRequest(query = "test"))
        assertTrue(result.isFailure)
        val msg = result.exceptionOrNull()?.message.orEmpty()
        assertTrue("failure message should contain 'HTTP 404'", msg.contains("HTTP 404"))
    }

    @Test
    fun `healthz returns false when server responds 500`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(500).setBody("error"))
        val result = client.healthz()
        assertTrue(result.isFailure)
    }

    // ── Empty response body ───────────────────────────────────────────────────

    @Test
    fun `ask returns failure when response body is empty`() = runBlocking {
        // 200 with an empty body — the execute() helper calls error("Empty response body")
        server.enqueue(MockResponse().setResponseCode(200).setBody(""))
        val result = client.ask(AskRequest(query = "hello"))
        assertTrue(result.isFailure)
        val msg = result.exceptionOrNull()?.message.orEmpty()
        assertTrue(
            "failure message should mention empty body or parse failure",
            msg.contains("Empty response body") || msg.contains("unexpected") || msg.isNotBlank(),
        )
    }

    @Test
    fun `query returns failure when response body is empty`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(200).setBody(""))
        val result = client.query(QueryRequest(query = "test"))
        assertTrue(result.isFailure)
    }

    // ── Network failure (connection refused) ─────────────────────────────────

    @Test
    fun `ask returns failure when server is unreachable`() = runBlocking {
        // Shut down the server before the call so the connection is refused.
        server.shutdown()
        val result = client.ask(AskRequest(query = "hello"))
        assertTrue(result.isFailure)
    }

    @Test
    fun `healthz returns false when server is unreachable`() = runBlocking {
        server.shutdown()
        val result = client.healthz()
        assertTrue(result.isFailure)
    }

    // ── updateConfig atomicity ────────────────────────────────────────────────

    @Test
    fun `updateConfig is observed by the next request`() = runBlocking {
        val server2 = MockWebServer()
        server2.start()
        try {
            server2.enqueue(
                MockResponse()
                    .setBody("""{"query":"hi","answer":"bye","timing_ms":null}""")
                    .addHeader("Content-Type", "application/json"),
            )

            val newBaseUrl = server2.url("/").toString().trimEnd('/')
            val newToken = "new-token"
            client.updateConfig(newBaseUrl, newToken)

            val result = client.ask(AskRequest(query = "hi"))
            assertTrue(result.isSuccess)

            // The request must have landed on server2, not the original server.
            assertEquals(0, server.requestCount)
            val req = server2.takeRequest(1, TimeUnit.SECONDS)
            checkNotNull(req) { "Expected a request on server2 after updateConfig" }
            assertEquals("Bearer $newToken", req.getHeader("Authorization"))
            assertEquals("$newToken", req.getHeader("x-api-key"))
        } finally {
            server2.shutdown()
        }
    }

    @Test
    fun `updateConfig trims trailing slash from baseUrl`() = runBlocking {
        val server2 = MockWebServer()
        server2.start()
        try {
            server2.enqueue(
                MockResponse()
                    .setBody("""{"query":"q","answer":"a","timing_ms":null}""")
                    .addHeader("Content-Type", "application/json"),
            )
            // Supply trailing slash — updateConfig must trim it so the path is correct.
            val urlWithSlash = server2.url("/").toString() // ends with "/"
            client.updateConfig(urlWithSlash, "tok")

            val result = client.ask(AskRequest(query = "q"))
            assertTrue(result.isSuccess)
            val req = server2.takeRequest(1, TimeUnit.SECONDS)
            checkNotNull(req)
            // Path must be "/v1/ask", not "//v1/ask".
            assertEquals("/v1/ask", req.path)
        } finally {
            server2.shutdown()
        }
    }

    // ── hasToken ──────────────────────────────────────────────────────────────

    @Test
    fun `hasToken returns true when token is non-blank`() {
        assertTrue(client.hasToken())
    }

    @Test
    fun `hasToken returns false when constructed with blank token`() {
        val emptyTokenClient = AxonClient(
            baseUrl = server.url("/").toString().trimEnd('/'),
            token = "",
        )
        assertFalse(emptyTokenClient.hasToken())
    }

    @Test
    fun `hasToken returns false when constructed with whitespace-only token`() {
        val whitespaceClient = AxonClient(
            baseUrl = server.url("/").toString().trimEnd('/'),
            token = "   ",
        )
        assertFalse(whitespaceClient.hasToken())
    }

    @Test
    fun `hasToken reflects updateConfig change`() {
        client.updateConfig(server.url("/").toString(), "")
        assertFalse(client.hasToken())

        client.updateConfig(server.url("/").toString(), "new-token")
        assertTrue(client.hasToken())
    }

    // ── Both auth headers are sent ────────────────────────────────────────────

    @Test
    fun `execute truncates oversized error body to 200 chars`() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(500).setBody("E".repeat(10_000)))
        val result = client.ask(AskRequest(query = "hello"))
        assertTrue(result.isFailure)
        val msg = result.exceptionOrNull()?.message.orEmpty()
        // "HTTP 500: " prefix + at most 200 body chars (no full 10k body)
        assertTrue("got message length ${msg.length}", msg.length <= 220)
    }

    @Test
    fun `execute converts JSON error body to human readable text`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setResponseCode(502)
                .setBody("""{"error":"LLM answer generation failed","request_id":"abc123"}""")
                .addHeader("Content-Type", "application/json"),
        )

        val result = client.ask(AskRequest(query = "hello"))

        assertTrue(result.isFailure)
        val msg = result.exceptionOrNull()?.message.orEmpty()
        assertEquals("HTTP 502: LLM answer generation failed", msg)
        assertFalse("message should not render raw JSON", msg.contains("{") || msg.contains("}") || msg.contains("\""))
    }

    @Test
    fun `ask sends both Authorization and x-api-key headers`() = runBlocking {
        server.enqueue(
            MockResponse()
                .setBody("""{"query":"q","answer":"a","timing_ms":null}""")
                .addHeader("Content-Type", "application/json"),
        )
        client.ask(AskRequest(query = "q"))
        val req = server.takeRequest()
        assertEquals("Bearer test-token", req.getHeader("Authorization"))
        assertEquals("test-token", req.getHeader("x-api-key"))
    }

    @Test
    fun `rest request uses oauth bearer and no x api key`() = runBlocking {
        val baseUrl = server.url("/").toString().trimEnd('/')
        client.updateConfig(baseUrl, AuthConfig.OAuth(FakeOAuthTokenSource(), baseUrl))
        server.enqueue(
            MockResponse()
                .setBody("""{"query":"q","answer":"a","timing_ms":null}""")
                .addHeader("Content-Type", "application/json"),
        )

        client.ask(AskRequest(query = "q"))

        val req = server.takeRequest()
        assertEquals("Bearer oauth-access-token", req.getHeader("Authorization"))
        assertEquals(null, req.getHeader("x-api-key"))
    }

    @Test
    fun `sse request uses oauth bearer and no x api key`() = runBlocking {
        val baseUrl = server.url("/").toString().trimEnd('/')
        client.updateConfig(baseUrl, AuthConfig.OAuth(FakeOAuthTokenSource(), baseUrl))
        server.enqueue(
            MockResponse()
                .setBody("data: {\"type\":\"done\",\"answer\":\"ok\"}\n\n")
                .addHeader("Content-Type", "text/event-stream"),
        )

        client.askStream(AskRequest(query = "ping")).toList()

        val req = server.takeRequest()
        assertEquals("Bearer oauth-access-token", req.getHeader("Authorization"))
        assertEquals(null, req.getHeader("x-api-key"))
    }

    @Test
    fun `panel route rejects oauth auth config before sending request`() = runBlocking {
        val baseUrl = server.url("/").toString().trimEnd('/')
        client.updateConfig(baseUrl, AuthConfig.OAuth(FakeOAuthTokenSource(), baseUrl))

        val result = client.panelEnv()

        assertTrue(result.isFailure)
        assertEquals(0, server.requestCount)
    }

    @Test
    fun `oauth credentials are rejected when base url changes`() = runBlocking {
        val oauthServerUrl = server.url("/").toString().trimEnd('/')
        val differentServer = MockWebServer()
        differentServer.start()
        try {
            client.updateConfig(
                differentServer.url("/").toString().trimEnd('/'),
                AuthConfig.OAuth(FakeOAuthTokenSource(), oauthServerUrl),
            )

            val result = client.ask(AskRequest(query = "q"))

            assertTrue(result.isFailure)
            assertEquals(0, differentServer.requestCount)
        } finally {
            differentServer.shutdown()
        }
    }

    @Test
    fun `bearer route still sends authorization and x api key`() = runBlocking {
        client.updateConfig(server.url("/").toString().trimEnd('/'), AuthConfig.Bearer("static-token"))
        server.enqueue(
            MockResponse()
                .setBody("""{"query":"q","answer":"a","timing_ms":null}""")
                .addHeader("Content-Type", "application/json"),
        )

        client.ask(AskRequest(query = "q"))

        val req = server.takeRequest()
        assertEquals("Bearer static-token", req.getHeader("Authorization"))
        assertEquals("static-token", req.getHeader("x-api-key"))
    }
}
