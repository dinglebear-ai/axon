package com.axon.app.data.auth

import android.content.Context
import android.util.Log
import com.axon.app.data.security.SecurePrefsFactory

private const val TAG = "OAuthStateStore"
private const val PREFS_NAME = "axon_oauth_state"
private const val KEY_AUTH_STATE = "auth_state_json"
private const val KEY_PENDING_STATE = "pending_authorization_state"

open class OAuthStateStore(context: Context) {
    private val appContext = context.applicationContext

    private fun createPrefs() = SecurePrefsFactory.create(appContext, PREFS_NAME)

    open fun read(): String? = runCatching {
        createPrefs().getString(KEY_AUTH_STATE, null)?.takeIf { it.isNotBlank() }
    }.recoverCatching { cause ->
        Log.w(TAG, "OAuth state read failed; clearing corrupted encrypted store", cause)
        clear()
        null
    }.getOrNull()

    open fun write(rawJson: String): Boolean = runCatching {
        createPrefs().edit().putString(KEY_AUTH_STATE, rawJson).commit()
    }.onFailure {
        Log.w(TAG, "OAuth state write failed", it)
    }.getOrDefault(false)

    open fun readPendingState(): String? = runCatching {
        createPrefs().getString(KEY_PENDING_STATE, null)?.takeIf { it.isNotBlank() }
    }.recoverCatching { cause ->
        Log.w(TAG, "OAuth pending state read failed; clearing corrupted pending state", cause)
        clearPendingState()
        null
    }.getOrNull()

    open fun writePendingState(state: String): Boolean = runCatching {
        createPrefs().edit().putString(KEY_PENDING_STATE, state).commit()
    }.onFailure {
        Log.w(TAG, "OAuth pending state write failed", it)
    }.getOrDefault(false)

    open fun clearPendingState(): Boolean = runCatching {
        createPrefs().edit().remove(KEY_PENDING_STATE).commit()
    }.onFailure {
        Log.w(TAG, "OAuth pending state clear failed", it)
    }.getOrDefault(false)

    open fun clear(): Boolean = runCatching {
        createPrefs().edit()
            .remove(KEY_AUTH_STATE)
            .remove(KEY_PENDING_STATE)
            .commit()
    }.onFailure {
        Log.w(TAG, "OAuth state clear failed", it)
    }.getOrDefault(false)
}
