package com.axon.app.core.api

import com.axon.app.core.auth.AuthConfig
import com.axon.app.core.auth.OAuthTokenSource
import com.axon.app.core.api.models.MobileChatItemDto
import com.axon.app.core.api.models.MobileSessionDto
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class GeneratedAxonApiTest {
    private lateinit var server: MockWebServer

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
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    @Test
    fun collectionsSendsBearerAndApiKeyHeaders() = runTest {
        server.enqueue(MockResponse().setResponseCode(200).setBody("""{"collections":["axon"]}"""))
        val api = api(AuthConfig.Bearer("secret-token"))

        val result = api.collections()

        assertTrue(result.isSuccess)
        assertEquals(listOf("axon"), result.getOrThrow().collections)
        val request = server.takeRequest()
        assertEquals("/v1/collections", request.path)
        assertEquals("Bearer secret-token", request.getHeader("Authorization"))
        assertEquals("secret-token", request.getHeader("x-api-key"))
        assertEquals(null, request.getHeader("x-axon-panel-token"))
    }

    @Test
    fun collectionsSendsOAuthBearerWithoutApiKey() = runTest {
        server.enqueue(MockResponse().setResponseCode(200).setBody("""{"collections":["axon"]}"""))
        val baseUrl = server.url("/").toString().trimEnd('/')
        val api = api(AuthConfig.OAuth(FakeOAuthTokenSource(), baseUrl))

        val result = api.collections()

        assertTrue(result.isSuccess)
        assertEquals(listOf("axon"), result.getOrThrow().collections)
        val request = server.takeRequest()
        assertEquals("/v1/collections", request.path)
        assertEquals("Bearer oauth-access-token", request.getHeader("Authorization"))
        assertEquals(null, request.getHeader("x-api-key"))
        assertEquals(null, request.getHeader("x-axon-panel-token"))
    }

    @Test
    fun collectionsRejectsOAuthCredentialsForDifferentServer() = runTest {
        val otherServerUrl = MockWebServer().use { other ->
            other.start()
            other.url("/").toString().trimEnd('/')
        }
        val api = api(AuthConfig.OAuth(FakeOAuthTokenSource(), otherServerUrl))

        val result = api.collections()

        assertTrue(result.isFailure)
        assertEquals(0, server.requestCount)
    }

    @Test
    fun generatedErrorsAreResultFailuresAndRedactTokens() = runTest {
        server.enqueue(
            MockResponse()
                .setResponseCode(401)
                .setBody(
                    """
                    {
                      "error":"nope",
                      "token":"secret-token",
                      "access_token":"oauth-access-token",
                      "Authorization":"Bearer secret-token",
                      "authorization":"bearer secret-token",
                      "url":"https://axon.test/callback?token=secret-token",
                      "nested":{"x-api-key":"secret-token"}
                    }
                    """.trimIndent(),
                )
        )
        val api = api(AuthConfig.Bearer("secret-token"))

        val result = api.collections()

        assertTrue(result.isFailure)
        val error = result.exceptionOrNull()
        val rendered = listOfNotNull(error?.message, error?.toString(), error?.cause?.toString()).joinToString("\n")
        assertTrue(rendered.contains("HTTP 401"))
        assertFalse(rendered.contains("secret-token"))
        assertFalse(rendered.contains("oauth-access-token"))
        assertFalse(rendered.contains("Bearer secret"))
        assertFalse(rendered.contains("bearer secret"))
        assertFalse(rendered.contains("token=secret-token"))
        assertEquals(null, error?.cause)
    }

    @Test
    fun mobileSessionsUseGeneratedClientAndMapSummaries() = runTest {
        server.enqueue(
            MockResponse()
                .setResponseCode(200)
                .setBody(
                    """
                    {
                      "sessions": [
                        {
                          "id": "session-1",
                          "title": "One",
                          "first_message_preview": "hello",
                          "turn_count": 2,
                          "injected_op_count": 1,
                          "created_at": 10,
                          "updated_at": 20,
                          "pinned_at": 15
                        }
                      ]
                    }
                    """.trimIndent(),
                ),
        )
        val api = api(AuthConfig.Bearer("secret-token"))

        val result = api.listMobileSessions()

        assertTrue(result.isSuccess)
        assertEquals(listOf("session-1"), result.getOrThrow().map { it.id })
        assertEquals(2, result.getOrThrow().single().turnCount)
        assertEquals(emptyList<MobileChatItemDto>(), result.getOrThrow().single().items)
        val request = server.takeRequest()
        assertEquals("/v1/mobile/sessions", request.path)
        assertEquals("GET", request.method)
    }

    @Test
    fun mobileSessionDetailMapsNestedPayload() = runTest {
        server.enqueue(
            MockResponse()
                .setResponseCode(200)
                .setBody(
                    """
                    {
                      "session": {
                        "id": "session-1",
                        "title": "One",
                        "first_message_preview": "hello",
                        "turn_count": 2,
                        "injected_op_count": 1,
                        "created_at": 10,
                        "updated_at": 20,
                        "items": [
                          {
                            "kind": "tool",
                            "text": "ran",
                            "payload": {"name":"source","ok":true},
                            "timestamp": 30
                          }
                        ]
                      }
                    }
                    """.trimIndent(),
                ),
        )
        val api = api(AuthConfig.Bearer("secret-token"))

        val result = api.getMobileSession("session-1")

        assertTrue(result.isSuccess)
        val item = result.getOrThrow().items.single()
        assertEquals("tool", item.kind)
        assertEquals("source", item.payload["name"]?.toString()?.trim('"'))
        assertEquals("true", item.payload["ok"]?.toString())
        val request = server.takeRequest()
        assertEquals("/v1/mobile/sessions/session-1", request.path)
        assertEquals("GET", request.method)
    }

    @Test
    fun mobileSessionUpsertSendsGeneratedBodyAndMapsResponse() = runTest {
        server.enqueue(
            MockResponse()
                .setResponseCode(200)
                .setBody(
                    """
                    {
                      "ok": true,
                      "session": {
                        "id": "session-1",
                        "title": "One",
                        "first_message_preview": "hello",
                        "turn_count": 1,
                        "injected_op_count": 0,
                        "created_at": 10,
                        "updated_at": 30,
                        "items": []
                      }
                    }
                    """.trimIndent(),
                ),
        )
        val api = api(AuthConfig.Bearer("secret-token"))
        val session = MobileSessionDto(
            id = "session-1",
            title = "One",
            firstMessagePreview = "hello",
            createdAt = 10,
            updatedAt = 20,
            items = listOf(
                MobileChatItemDto(
                    kind = "user",
                    text = "hello",
                    payload = buildJsonObject { put("source", "test") },
                    timestamp = 11,
                ),
            ),
        )

        val result = api.upsertMobileSession(session)

        assertTrue(result.isSuccess)
        assertEquals(30, result.getOrThrow().updatedAt)
        val request = server.takeRequest()
        assertEquals("/v1/mobile/sessions/session-1", request.path)
        assertEquals("PUT", request.method)
        val body = request.body.readUtf8()
        assertTrue(body.contains(""""source":"test""""))
        assertTrue(body.contains(""""first_message_preview":"hello""""))
    }

    @Test
    fun mobileSessionDeleteUsesGeneratedClient() = runTest {
        server.enqueue(MockResponse().setResponseCode(200).setBody("""{"ok":true}"""))
        val api = api(AuthConfig.Bearer("secret-token"))

        val result = api.deleteMobileSession("session-1")

        assertTrue(result.isSuccess)
        assertTrue(result.getOrThrow())
        val request = server.takeRequest()
        assertEquals("/v1/mobile/sessions/session-1", request.path)
        assertEquals("DELETE", request.method)
    }

    private fun api(auth: AuthConfig): GeneratedAxonApi =
        GeneratedAxonApi(
            snapshotProvider = { ClientAuthSnapshot(server.url("/").toString().trimEnd('/'), auth) },
            clients = AxonHttpClients(),
        )
}
