package com.axon.app.data.auth

import android.content.Context
import android.util.Log
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey

private const val TAG = "OAuthStateStore"
private const val PREFS_NAME = "axon_oauth_state"
private const val KEY_AUTH_STATE = "auth_state_json"

open class OAuthStateStore(context: Context) {
    private val appContext = context.applicationContext

    private fun createPrefs() = EncryptedSharedPreferences.create(
        appContext,
        PREFS_NAME,
        MasterKey.Builder(appContext)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build(),
        EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
        EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
    )

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

    open fun clear(): Boolean = runCatching {
        createPrefs().edit().remove(KEY_AUTH_STATE).commit()
    }.onFailure {
        Log.w(TAG, "OAuth state clear failed", it)
    }.getOrDefault(false)
}
