package com.axon.app.core.api

import com.axon.app.core.auth.AuthConfig
import com.axon.app.core.auth.MissingAuthException
import okhttp3.Request

internal data class ClientAuthSnapshot(
    val baseUrl: String,
    val auth: AuthConfig,
)

internal fun Request.Builder.withApiAuth(token: String): Request.Builder =
    header("Authorization", "Bearer $token")
        .header("x-api-key", token)

internal suspend fun ClientAuthSnapshot.authHeaders(panelRoute: Boolean = false): Map<String, String> {
    if (panelRoute && auth is AuthConfig.OAuth) {
        throw MissingAuthException("Server config requires bearer/panel-compatible auth; OAuth app tokens are not used for panel routes")
    }

    return when (auth) {
        is AuthConfig.Bearer -> {
            val token = auth.token.trim()
            if (token.isBlank()) throw MissingAuthException("No Axon authentication configured")
            if (panelRoute) {
                mapOf("x-axon-panel-token" to token)
            } else {
                mapOf("Authorization" to "Bearer $token", "x-api-key" to token)
            }
        }
        is AuthConfig.OAuth -> {
            if (auth.serverUrl.trimEnd('/') != baseUrl.trimEnd('/')) {
                throw MissingAuthException("OAuth credentials belong to a different Axon server; sign in again for this server")
            }
            val token = auth.tokenSource.freshAccessToken().getOrThrow()
            mapOf("Authorization" to "Bearer $token")
        }
    }
}

internal suspend fun Request.Builder.withAxonAuth(
    snapshot: ClientAuthSnapshot,
    panelRoute: Boolean = false,
): Request.Builder {
    snapshot.authHeaders(panelRoute).forEach { (name, value) -> header(name, value) }
    return this
}
