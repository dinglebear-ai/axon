package com.axon.app.data.auth

enum class AuthMode {
    Bearer,
    OAuth,
}

fun AuthMode.toWireValue(): String = when (this) {
    AuthMode.Bearer -> "bearer"
    AuthMode.OAuth -> "oauth"
}

fun authModeFromWireValue(value: String?): AuthMode = when (value?.trim()?.lowercase()) {
    null, "" -> AuthMode.OAuth
    "bearer" -> AuthMode.Bearer
    "oauth" -> AuthMode.OAuth
    else -> AuthMode.Bearer
}
