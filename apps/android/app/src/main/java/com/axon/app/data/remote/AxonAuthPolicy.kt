package com.axon.app.data.remote

import okhttp3.Request

internal const val PANEL_UNLOCK_REQUIRED_MESSAGE =
    "Panel unlock required. Save the panel password before loading or editing panel config."

internal fun Request.Builder.withApiAuth(token: String): Request.Builder =
    header("Authorization", "Bearer $token")
        .header("x-api-key", token)

internal fun Request.Builder.withPanelAuth(token: String): Request.Builder =
    header("Authorization", "Bearer $token")
        .header("x-axon-panel-token", token)

internal fun requirePanelToken(token: String): Result<String> {
    val trimmed = token.trim()
    return if (trimmed.isBlank()) {
        Result.failure(IllegalStateException(PANEL_UNLOCK_REQUIRED_MESSAGE))
    } else {
        Result.success(trimmed)
    }
}
