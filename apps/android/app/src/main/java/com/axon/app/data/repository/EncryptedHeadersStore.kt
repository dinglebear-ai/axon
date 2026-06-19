package com.axon.app.data.repository

import android.content.Context
import android.util.Log
import com.axon.app.data.security.SecurePrefsFactory

/**
 * Encrypted storage for user-configured HTTP headers.
 *
 * Header values can carry bearer tokens, cookies, and API keys (see
 * [com.axon.app.ui.options.components.SENSITIVE_HEADER_KEYS]). Persisting them in
 * the plaintext mode-options DataStore would defeat [EncryptedTokenStore] — any
 * `Authorization: Bearer …` line in a crawl form would land on disk in the clear.
 *
 * This store uses the same AES-256-GCM scheme as [EncryptedTokenStore]. Header
 * lists are persisted per *mode key* (Crawl headers, Ask headers, etc.) as a
 * newline-joined `"Key: Value"` string — matching the wire format the server
 * expects in `headers: Vec<String>`.
 *
 * Tolerates AndroidKeyStore invalidation the same way [EncryptedTokenStore] does:
 * on AEAD failure the file is deleted and `null` is returned so callers force the
 * user to re-enter their headers rather than crashing.
 */
class EncryptedHeadersStore(private val context: Context) {
    private val prefs by lazy {
        runCatching {
            SecurePrefsFactory.create(context, FILE)
        }.getOrElse { t ->
            Log.w(TAG, "EncryptedSharedPreferences init failed for $FILE; clearing file", t)
            context.deleteSharedPreferences(FILE)
            null
        }
    }

    fun read(key: String): List<String>? {
        val p = prefs ?: return null
        val raw = runCatching { p.getString(key, null) }
            .getOrElse { t ->
                Log.w(TAG, "EncryptedSharedPreferences read failed for $key; clearing file", t)
                context.deleteSharedPreferences(FILE)
                null
            }
            ?: return null
        if (raw.isEmpty()) return emptyList()
        return raw.split("\n").filter { it.isNotBlank() }
    }

    /** Synchronous commit so credentials survive an immediate process kill. */
    fun write(key: String, headers: List<String>) {
        val p = prefs ?: run {
            Log.w(TAG, "EncryptedSharedPreferences unavailable; cannot persist header set for $key")
            return
        }
        @Suppress("ApplySharedPref")
        val ok = p.edit().putString(key, headers.joinToString("\n")).commit()
        if (!ok) Log.w(TAG, "commit() returned false when writing $key")
    }

    fun clear(key: String) {
        val ok = prefs?.edit()?.remove(key)?.commit() ?: false
        if (!ok) Log.w(TAG, "commit() returned false when clearing $key")
    }

    companion object {
        private const val TAG = "EncryptedHeadersStore"
        private const val FILE = "axon_headers"

        // Canonical keys used across mode-options forms. Add new modes here when
        // their forms accept headers.
        const val KEY_CRAWL_HEADERS = "crawl.headers"
    }
}
