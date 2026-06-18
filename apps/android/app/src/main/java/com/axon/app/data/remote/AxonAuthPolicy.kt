package com.axon.app.data.remote

import okhttp3.Request

internal fun Request.Builder.withApiAuth(token: String): Request.Builder =
    header("Authorization", "Bearer $token")
        .header("x-api-key", token)
