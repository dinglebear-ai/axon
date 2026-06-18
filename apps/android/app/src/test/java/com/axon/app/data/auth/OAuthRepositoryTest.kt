package com.axon.app.data.auth

import android.content.Context
import android.content.Intent
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.runBlocking
import net.openid.appauth.AuthState
import net.openid.appauth.AuthorizationException
import net.openid.appauth.AuthorizationRequest
import net.openid.appauth.AuthorizationResponse
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.json.JSONObject

@RunWith(AndroidJUnit4::class)
@Config(sdk = [33])
class OAuthRepositoryTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private lateinit var server: MockWebServer
    private lateinit var store: InMemoryOAuthStateStore
    private lateinit var repo: OAuthRepository

    @Before
    fun setUp() {
        server = MockWebServer()
        server.start()
        store = InMemoryOAuthStateStore(context)
        repo = OAuthRepository(context, store)
    }

    @After
    fun tearDown() {
        repo.dispose()
        server.shutdown()
    }

    @Test
    fun `redactOAuthError removes oauth secrets`() {
        val raw = "access_token=a refresh_token=b code=c client_secret=d"
        val redacted = redactOAuthError(raw)
        assertFalse(redacted.contains("=a"))
        assertFalse(redacted.contains("=b"))
        assertFalse(redacted.contains("=c"))
        assertFalse(redacted.contains("=d"))
        assertTrue(redacted.contains("access_token=<redacted>"))
    }

    @Test
    fun `signOut clears persisted auth state`() = runBlocking {
        assertTrue(store.write("""{"config":{}}"""))
        assertTrue(store.writePendingState("pending"))

        assertTrue(repo.signOut())
        assertFalse(repo.isSignedIn())
        assertNull(store.read())
        assertNull(store.readPendingState())
    }

    @Test
    fun `discovery registration and authorization request have axon native-client shape`() = runBlocking {
        enqueueDiscoveryAndRegistration()

        val intent = repo.createAuthorizationRequest(baseUrl())
        val request = authorizationRequestFrom(intent)

        assertEquals("/.well-known/oauth-authorization-server", server.takeRequest().path)
        val registration = server.takeRequest()
        val registrationBody = registration.body.readUtf8()
        val registrationJson = JSONObject(registrationBody)
        assertEquals("com.axon.app://oauth2redirect", registrationJson.getJSONArray("redirect_uris").getString(0))
        assertEquals("none", registrationJson.getString("token_endpoint_auth_method"))

        val authUri = request.toUri().toString()
        assertTrue(authUri.contains("axon%3Aread") || authUri.contains("axon:read"))
        assertTrue(authUri.contains("axon%3Awrite") || authUri.contains("axon:write"))
        assertFalse(authUri.contains("offline_access"))
        assertEquals("com.axon.app://oauth2redirect", request.redirectUri.toString())
        assertNotNull(store.read())
        assertEquals(request.state, store.readPendingState())
    }

    @Test
    fun `authorization callback survives repository recreation`() = runBlocking {
        enqueueDiscoveryAndRegistration()
        val request = authorizationRequestFrom(repo.createAuthorizationRequest(baseUrl()))
        server.takeRequest()
        server.takeRequest()
        repo.dispose()
        repo = OAuthRepository(context, store)
        server.enqueue(tokenResponse("access-after-recreate", "refresh-after-recreate"))

        val result = repo.handleAuthorizationResponse(callbackIntent(request))

        assertTrue(result.isSuccess)
        val tokenExchange = server.takeRequest()
        val tokenExchangeBody = tokenExchange.body.readUtf8()
        assertEquals("/token", tokenExchange.path)
        assertTrue(tokenExchangeBody.contains("grant_type=authorization_code"))
        assertTrue(tokenExchangeBody.contains("code=auth-code"))
        assertTrue(tokenExchangeBody.contains("client_id=android-client"))
        assertEquals("access-after-recreate", AuthState.jsonDeserialize(checkNotNull(store.read())).accessToken)
        assertNull(store.readPendingState())
    }

    @Test
    fun `token exchange persists only after successful token response`() = runBlocking {
        enqueueDiscoveryAndRegistration()
        val request = authorizationRequestFrom(repo.createAuthorizationRequest(baseUrl()))
        server.takeRequest()
        server.takeRequest()
        server.enqueue(jsonResponse("""{"error":"invalid_grant"}""", 400))

        val failed = repo.handleAuthorizationResponse(callbackIntent(request, code = "bad-code"))
        assertTrue(failed.isFailure)
        assertFalse(AuthState.jsonDeserialize(checkNotNull(store.read())).isAuthorized)

        repo.signOut()
        enqueueDiscoveryAndRegistration()
        val retryRequest = authorizationRequestFrom(repo.createAuthorizationRequest(baseUrl()))
        server.takeRequest()
        server.takeRequest()
        server.enqueue(tokenResponse("access-ok", "refresh-ok"))

        val succeeded = repo.handleAuthorizationResponse(callbackIntent(retryRequest, code = "good-code"))
        assertTrue(succeeded.isSuccess)
        assertTrue(AuthState.jsonDeserialize(checkNotNull(store.read())).isAuthorized)
        assertEquals("access-ok", AuthState.jsonDeserialize(checkNotNull(store.read())).accessToken)
    }

    @Test
    fun `ten concurrent freshAccessToken calls are single flight`() = runBlocking {
        enqueueDiscoveryAndRegistration()
        val request = authorizationRequestFrom(repo.createAuthorizationRequest(baseUrl()))
        server.takeRequest()
        server.takeRequest()
        server.enqueue(tokenResponse("old-access", "refresh-token", expiresIn = 1))
        assertTrue(repo.handleAuthorizationResponse(callbackIntent(request)).isSuccess)
        server.takeRequest()

        val staleState = AuthState.jsonDeserialize(checkNotNull(store.read())).also { it.setNeedsTokenRefresh(true) }
        assertTrue(store.write(staleState.jsonSerializeString()))
        server.enqueue(tokenResponse("fresh-access", "refresh-token"))

        val results = (1..10).map {
            async { repo.freshAccessToken() }
        }.awaitAll()

        assertTrue(results.all { it.getOrNull() == "fresh-access" })
        val refresh = server.takeRequest()
        val refreshBody = refresh.body.readUtf8()
        assertTrue(refreshBody.contains("grant_type=refresh_token"))
        assertTrue(refreshBody.contains("client_id=android-client"))
        assertEquals(4, server.requestCount)
    }

    @Test
    fun `refresh failure asks user to sign in again and redacts token words`() = runBlocking {
        enqueueDiscoveryAndRegistration()
        val request = authorizationRequestFrom(repo.createAuthorizationRequest(baseUrl()))
        server.takeRequest()
        server.takeRequest()
        server.enqueue(tokenResponse("old-access", "refresh-token", expiresIn = 1))
        assertTrue(repo.handleAuthorizationResponse(callbackIntent(request)).isSuccess)
        server.takeRequest()

        val staleState = AuthState.jsonDeserialize(checkNotNull(store.read())).also { it.setNeedsTokenRefresh(true) }
        assertTrue(store.write(staleState.jsonSerializeString()))
        server.enqueue(jsonResponse("""{"error":"invalid_grant","error_description":"access_token=a refresh_token=b client_secret=c code=d"}""", 400))

        val result = repo.freshAccessToken()

        assertTrue(result.isFailure)
        val message = result.exceptionOrNull()?.message.orEmpty()
        assertTrue(message.contains("sign in"))
        assertFalse(message.contains("access_token=a"))
        assertFalse(message.contains("refresh_token=b"))
        assertFalse(message.contains("client_secret=c"))
        assertFalse(message.contains("code=d"))
    }

    @Test
    fun `wrong missing error replayed and signed-out callbacks fail`() = runBlocking {
        enqueueDiscoveryAndRegistration()
        val request = authorizationRequestFrom(repo.createAuthorizationRequest(baseUrl()))
        server.takeRequest()
        server.takeRequest()

        assertTrue(repo.handleAuthorizationResponse(callbackIntent(request, state = "wrong-state")).isFailure)
        assertTrue(repo.handleAuthorizationResponse(callbackIntent(request, code = null)).isFailure)

        enqueueDiscoveryAndRegistration()
        val retryRequest = authorizationRequestFrom(repo.createAuthorizationRequest(baseUrl()))
        server.takeRequest()
        server.takeRequest()
        val error = AuthorizationException.fromOAuthRedirect(
            retryRequest.redirectUri.buildUpon()
                .appendQueryParameter("error", "access_denied")
                .appendQueryParameter("state", retryRequest.state)
                .build(),
        ).toIntent()
        val errorResult = repo.handleAuthorizationResponse(error)
        assertTrue(errorResult.isFailure)
        assertTrue(errorResult.exceptionOrNull()?.message.orEmpty().contains("access_denied"))

        enqueueDiscoveryAndRegistration()
        val successRequest = authorizationRequestFrom(repo.createAuthorizationRequest(baseUrl()))
        server.takeRequest()
        server.takeRequest()
        val successIntent = callbackIntent(successRequest)
        server.enqueue(tokenResponse("access", "refresh"))
        assertTrue(repo.handleAuthorizationResponse(successIntent).isSuccess)
        server.takeRequest()
        assertTrue(repo.handleAuthorizationResponse(successIntent).isFailure)

        enqueueDiscoveryAndRegistration()
        val signedOutRequest = authorizationRequestFrom(repo.createAuthorizationRequest(baseUrl()))
        server.takeRequest()
        server.takeRequest()
        repo.signOut()
        assertTrue(repo.handleAuthorizationResponse(callbackIntent(signedOutRequest)).isFailure)
    }

    @Test
    fun `persistence failure fails sign in`() = runBlocking {
        val failingRepo = OAuthRepository(context, InMemoryOAuthStateStore(context, failWrites = true))
        enqueueDiscoveryAndRegistration()

        val result = runCatching { failingRepo.createAuthorizationRequest(baseUrl()) }

        assertTrue(result.isFailure)
        assertTrue(result.exceptionOrNull()?.message.orEmpty().contains("securely store"))
        failingRepo.dispose()
    }

    private fun enqueueDiscoveryAndRegistration() {
        server.enqueue(jsonResponse(discoveryJson()))
        server.enqueue(jsonResponse("""{"client_id":"android-client","token_endpoint_auth_method":"none"}"""))
    }

    private fun baseUrl(): String = server.url("/").toString().trimEnd('/')

    private fun discoveryJson(): String = """
        {
          "issuer": "${baseUrl()}",
          "authorization_endpoint": "${baseUrl()}/authorize",
          "token_endpoint": "${baseUrl()}/token",
          "jwks_uri": "${baseUrl()}/jwks",
          "registration_endpoint": "${baseUrl()}/register",
          "subject_types_supported": ["public"],
          "id_token_signing_alg_values_supported": ["RS256"],
          "response_types_supported": ["code"],
          "grant_types_supported": ["authorization_code", "refresh_token"],
          "code_challenge_methods_supported": ["S256"]
        }
    """.trimIndent()

    private fun tokenResponse(accessToken: String, refreshToken: String, expiresIn: Long = 3600): MockResponse =
        jsonResponse(
            """
                {
                  "access_token": "$accessToken",
                  "refresh_token": "$refreshToken",
                  "expires_in": $expiresIn,
                  "token_type": "Bearer",
                  "scope": "axon:read axon:write"
                }
            """.trimIndent(),
        )

    private fun jsonResponse(body: String, code: Int = 200): MockResponse =
        MockResponse()
            .setResponseCode(code)
            .addHeader("Content-Type", "application/json")
            .setBody(body)

    private fun authorizationRequestFrom(intent: Intent): AuthorizationRequest =
        AuthorizationRequest.jsonDeserialize(checkNotNull(intent.getStringExtra("authRequest")))

    private fun callbackIntent(
        request: AuthorizationRequest,
        code: String? = "auth-code",
        state: String? = request.state,
    ): Intent {
        val response = AuthorizationResponse.Builder(request)
            .setState(state)
            .apply {
                if (code != null) setAuthorizationCode(code)
            }
            .build()
        return response.toIntent()
    }

    private class InMemoryOAuthStateStore(
        context: Context,
        private val failWrites: Boolean = false,
    ) : OAuthStateStore(context) {
        private var value: String? = null
        private var pendingValue: String? = null

        override fun read(): String? = value

        override fun write(rawJson: String): Boolean {
            if (failWrites) return false
            value = rawJson
            return true
        }

        override fun readPendingState(): String? = pendingValue

        override fun writePendingState(state: String): Boolean {
            if (failWrites) return false
            pendingValue = state
            return true
        }

        override fun clearPendingState(): Boolean {
            pendingValue = null
            return true
        }

        override fun clear(): Boolean {
            value = null
            pendingValue = null
            return true
        }
    }
}
