package com.axon.app.core.auth

import android.content.Context
import android.content.Intent
import android.net.Uri
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import net.openid.appauth.AuthState
import net.openid.appauth.AuthorizationException
import net.openid.appauth.AuthorizationManagementActivity
import net.openid.appauth.AuthorizationRequest
import net.openid.appauth.AuthorizationResponse
import net.openid.appauth.AuthorizationService
import net.openid.appauth.AuthorizationServiceConfiguration
import net.openid.appauth.ClientAuthentication
import net.openid.appauth.NoClientAuthentication
import net.openid.appauth.RegistrationRequest
import net.openid.appauth.RegistrationResponse
import net.openid.appauth.ResponseTypeValues
import net.openid.appauth.TokenRequest
import net.openid.appauth.TokenResponse
import okhttp3.FormBody
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException

private val AXON_OAUTH_REDIRECT_URI: Uri = Uri.parse("com.axon.app://oauth2redirect")
private const val AXON_OAUTH_SCOPE = "axon:read axon:write"
private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()

class OAuthRepository(
    context: Context,
    private val stateStore: OAuthStateStore,
) : OAuthTokenSource {
    private val appContext = context.applicationContext
    private val service = AuthorizationService(appContext)
    private val http = OkHttpClient()
    private val mutex = Mutex()
    private var authState: AuthState? = null
    private var signInInFlight = false
    private var pendingAuthorizationState: String? = null

    override fun isSignedIn(): Boolean = runCatching { loadedState().isAuthorized }.getOrDefault(false)

    suspend fun createAuthorizationRequest(baseUrl: String): Intent = withContext(Dispatchers.IO) {
        mutex.withLock {
            if (signInInFlight) error("OAuth sign-in already in progress")
            signInInFlight = true
        }
        runCatching {
            val serviceConfig = discover(baseUrl)
            val registration = register(serviceConfig)
            val request = AuthorizationRequest.Builder(
                serviceConfig,
                registration.clientId,
                ResponseTypeValues.CODE,
                AXON_OAUTH_REDIRECT_URI,
            )
                .setScope(AXON_OAUTH_SCOPE)
                .build()
            val requestState = request.state ?: error("OAuth authorization request did not include state")
            mutex.withLock {
                val state = AuthState(serviceConfig).also { it.update(registration) }
                persistOrThrow(state)
                authState = state
                pendingAuthorizationState = requestState
                persistPendingStateOrThrow(requestState)
            }
            authorizationIntentFor(request)
        }.onFailure {
            mutex.withLock {
                signInInFlight = false
                clearPendingAuthorizationState()
            }
        }.getOrThrow()
    }

    suspend fun handleAuthorizationResponse(intent: Intent): Result<Unit> = withContext(Dispatchers.IO) {
        val response = AuthorizationResponse.fromIntent(intent)
        val exception = AuthorizationException.fromIntent(intent)
        val expectedState = mutex.withLock {
            pendingAuthorizationState ?: stateStore.readPendingState()
        }
        if (response == null) {
            mutex.withLock {
                signInInFlight = false
                clearPendingAuthorizationState()
            }
            return@withContext Result.failure(
                exception?.let {
                    IllegalStateException(
                        "OAuth sign-in failed: ${redactOAuthError(it.error ?: it.errorDescription ?: it.message.orEmpty())}",
                    )
                } ?: IllegalStateException("OAuth response missing"),
            )
        }
        if (expectedState == null || expectedState != response.state) {
            mutex.withLock {
                signInInFlight = false
                clearPendingAuthorizationState()
            }
            return@withContext Result.failure(IllegalStateException("OAuth callback state was invalid; please sign in again"))
        }
        val result = runCatching {
            val state = loadedState()
            state.update(response, exception)
            val tokenResponse = performTokenRequest(response.createTokenExchangeRequest(), clientAuthenticationFor(state))
            state.update(tokenResponse, null)
            persistOrThrow(state)
            authState = state
            Unit
        }
        mutex.withLock {
            signInInFlight = false
            clearPendingAuthorizationState()
        }
        result
    }

    override suspend fun freshAccessToken(): Result<String> = withContext(Dispatchers.IO) {
        mutex.withLock {
            runCatching {
                val state = loadedState()
                val before = state.jsonSerializeString()
                if (state.needsTokenRefresh) {
                    val refreshResponse = performTokenRequest(state.createTokenRefreshRequest(), clientAuthenticationFor(state))
                    state.update(refreshResponse, null)
                }
                val token = state.accessToken
                    ?.takeIf { it.isNotBlank() }
                    ?: throw MissingAuthException("OAuth access token unavailable; please sign in again")
                val after = state.jsonSerializeString()
                if (after != before) persistOrThrow(state)
                authState = state
                token
            }.recoverCatching { cause ->
                throw IllegalStateException(
                    "OAuth session expired or could not refresh; please sign in again: ${redactOAuthError(cause.message.orEmpty())}",
                )
            }
        }
    }

    suspend fun cancelSignIn(): Boolean = withContext(Dispatchers.IO) {
        mutex.withLock {
            signInInFlight = false
            clearPendingAuthorizationState()
        }
    }

    suspend fun signOut(): Boolean = withContext(Dispatchers.IO) {
        mutex.withLock {
            authState = AuthState()
            signInInFlight = false
            pendingAuthorizationState = null
            stateStore.clear()
        }
    }

    fun dispose() {
        service.dispose()
    }

    private fun loadedState(): AuthState {
        authState?.let { return it }
        val loaded = stateStore.read()?.let { AuthState.jsonDeserialize(it) } ?: AuthState()
        authState = loaded
        return loaded
    }

    private fun persistOrThrow(state: AuthState) {
        if (!stateStore.write(state.jsonSerializeString())) {
            throw IllegalStateException("Could not securely store OAuth credentials")
        }
    }

    private fun persistPendingStateOrThrow(state: String) {
        if (!stateStore.writePendingState(state)) {
            throw IllegalStateException("Could not securely store OAuth pending state")
        }
    }

    private fun clearPendingAuthorizationState(): Boolean {
        pendingAuthorizationState = null
        return stateStore.clearPendingState()
    }

    private suspend fun discover(baseUrl: String): AuthorizationServiceConfiguration =
        withContext(Dispatchers.IO) {
            val metadataUrl = Uri.parse("${baseUrl.trimEnd('/')}/.well-known/oauth-authorization-server")
            val raw = executeHttp(
                Request.Builder()
                    .url(metadataUrl.toString())
                    .get()
                    .build(),
                "OAuth discovery failed",
            )
            val metadata = JSONObject(raw)
            AuthorizationServiceConfiguration(
                Uri.parse(metadata.getString("authorization_endpoint")),
                Uri.parse(metadata.getString("token_endpoint")),
                metadata.optString("registration_endpoint").takeIf { it.isNotBlank() }?.let(Uri::parse),
            )
        }

    private suspend fun register(config: AuthorizationServiceConfiguration): RegistrationResponse =
        withContext(Dispatchers.IO) {
            val request = RegistrationRequest.Builder(config, listOf(AXON_OAUTH_REDIRECT_URI))
                .setTokenEndpointAuthenticationMethod("none")
                .build()
            val raw = executeHttp(
                Request.Builder()
                    .url(config.registrationEndpoint.toString())
                    .post(request.jsonSerializeString().toRequestBody(JSON_MEDIA_TYPE))
                    .build(),
                "OAuth registration failed",
            )
            RegistrationResponse.Builder(request)
                .fromResponseJson(JSONObject(raw))
                .build()
        }

    private fun clientAuthenticationFor(state: AuthState): ClientAuthentication =
        runCatching { state.clientAuthentication }.getOrDefault(NoClientAuthentication.INSTANCE)

    private fun authorizationIntentFor(request: AuthorizationRequest): Intent =
        runCatching { service.getAuthorizationRequestIntent(request) }
            .getOrElse {
                AuthorizationManagementActivity.createStartForResultIntent(
                    appContext,
                    request,
                    Intent(Intent.ACTION_VIEW, request.toUri()),
                )
            }

    private fun performTokenRequest(
        tokenRequest: TokenRequest,
        clientAuthentication: ClientAuthentication,
    ): TokenResponse {
        val requestParams = LinkedHashMap(tokenRequest.getRequestParameters())
        clientAuthentication.getRequestParameters(tokenRequest.clientId).orEmpty().forEach { (name, value) ->
            requestParams[name] = value
        }
        val form = FormBody.Builder().apply {
            requestParams.forEach { (name, value) -> add(name, value) }
        }.build()
        val request = Request.Builder()
            .url(tokenRequest.configuration.tokenEndpoint.toString())
            .post(form)
            .apply {
                clientAuthentication.getRequestHeaders(tokenRequest.clientId).orEmpty().forEach { (name, value) ->
                    header(name, value)
                }
            }
            .build()
        val raw = executeHttp(
            request,
            "OAuth token request failed; please sign in again",
        )
        return TokenResponse.Builder(tokenRequest)
            .fromResponseJsonString(raw)
            .build()
    }

    private fun executeHttp(request: Request, failurePrefix: String): String {
        http.newCall(request).execute().use { response ->
            val body = response.body?.string().orEmpty()
            if (!response.isSuccessful) {
                val detail = redactedOAuthErrorFromBody(body).ifBlank { response.message }
                throw IOException("$failurePrefix: $detail")
            }
            return body
        }
    }
}

internal fun redactOAuthError(raw: String): String =
    raw.replace(Regex("(?i)(access_token|refresh_token|code|client_secret)=([^&\\s]+)"), "$1=<redacted>")

private fun redactedOAuthErrorFromBody(body: String): String {
    val parsed = runCatching { JSONObject(body) }.getOrNull() ?: return redactOAuthError(body)
    val fields = listOf("error", "error_description")
        .mapNotNull { key -> parsed.optString(key).takeIf { it.isNotBlank() } }
    return redactOAuthError(fields.joinToString(": "))
}
