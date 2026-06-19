package com.axon.app.data.remote

import com.axon.app.data.auth.AuthConfig
import com.axon.app.data.auth.OAuthTokenSource
import kotlinx.coroutines.test.runTest
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
        server.enqueue(MockResponse().setResponseCode(401).setBody("""{"error":"nope","token":"secret-token"}"""))
        val api = api(AuthConfig.Bearer("secret-token"))

        val result = api.collections()

        assertTrue(result.isFailure)
        val message = result.exceptionOrNull()?.message.orEmpty()
        assertTrue(message.contains("HTTP 401"))
        assertFalse(message.contains("secret-token"))
        assertFalse(message.contains("Authorization"))
        assertFalse(message.contains("x-api-key"))
    }

    private fun api(auth: AuthConfig): GeneratedAxonApi =
        GeneratedAxonApi(
            baseUrlProvider = { server.url("/").toString().trimEnd('/') },
            authProvider = { server.url("/").toString().trimEnd('/') to auth },
            clients = AxonHttpClients(),
        )
}
