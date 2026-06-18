package com.axon.app.data.auth

class MissingAuthException(message: String) : IllegalStateException(message)

interface OAuthTokenSource {
    suspend fun freshAccessToken(): Result<String>
    fun isSignedIn(): Boolean
}

sealed interface AuthConfig {
    data class Bearer(val token: String) : AuthConfig
    data class OAuth(val tokenSource: OAuthTokenSource) : AuthConfig
}

fun AuthConfig.hasUsableAuth(): Boolean = when (this) {
    is AuthConfig.Bearer -> token.trim().isNotBlank()
    is AuthConfig.OAuth -> tokenSource.isSignedIn()
}
