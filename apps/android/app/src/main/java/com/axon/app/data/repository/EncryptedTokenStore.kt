package com.axon.app.data.repository

import android.content.Context
import android.util.Log
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey

/**
 * Encrypted storage for the bearer token. Tolerates AndroidKeyStore invalidation
 * (biometric re-enroll, factory-restore, device-admin wipe) by clearing the
 * shared-prefs file on the first decryption failure and surfacing re-auth.
 *
 * Decrypted token is cached in a @Volatile so repeated authed calls don't
 * round-trip the keystore HAL.
 */
class EncryptedTokenStore(private val context: Context) {
    @Volatile private var cached: String? = null

    private val prefs by lazy {
        runCatching {
            EncryptedSharedPreferences.create(
                context,
                FILE,
                MasterKey.Builder(context).setKeyScheme(MasterKey.KeyScheme.AES256_GCM).build(),
                EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
            )
        }.getOrElse { t ->
            // Master key invalidated or shared-prefs file corrupted (e.g. AEADBadTagException
            // raised during EncryptedSharedPreferences.create()). Delete the file and surface
            // null so callers force re-auth instead of crashing on every read.
            Log.w(TAG, "EncryptedSharedPreferences init failed for $FILE; clearing file and forcing re-auth", t)
            context.deleteSharedPreferences(FILE)
            null
        }
    }

    fun read(): String? {
        // If prefs was invalidated and cleared earlier in this process, drop the
        // cached value so we don't keep handing out a token that's no longer on
        // disk. Cache stays warm in the happy path (cached != null && prefs != null).
        if (prefs == null) {
            if (cached != null) {
                Log.w(TAG, "prefs unavailable; clearing in-memory cache")
                cached = null
            }
            return null
        }
        cached?.let { return it }
        val p = prefs ?: return null
        return runCatching { p.getString(KEY_TOKEN, null) }
            .getOrElse { t ->
                // Master key invalidated at read time; clear and force re-auth.
                Log.w(TAG, "EncryptedSharedPreferences read failed; clearing file and forcing re-auth", t)
                context.deleteSharedPreferences(FILE)
                cached = null
                null
            }
            .also { cached = it }
    }

    /**
     * Synchronous commit — credentials must survive immediate process kill.
     *
     * Returns true on success, false if encrypted prefs are unavailable
     * (init failed) or `commit()` returned false. Callers should treat false
     * as "token NOT persisted" and prompt the user to retry.
     */
    fun write(token: String): Boolean {
        val p = prefs ?: run {
            Log.w(TAG, "write() failed: EncryptedSharedPreferences unavailable")
            return false
        }
        @Suppress("ApplySharedPref")
        val ok = p.edit().putString(KEY_TOKEN, token).commit()
        if (!ok) {
            Log.w(TAG, "write() commit() returned false; token NOT persisted")
            return false
        }
        cached = token
        return true
    }

    /**
     * Synchronous commit — must succeed before the caller treats the token as
     * cleared. Returns true on success, false otherwise (callers should
     * consider the on-disk token still present and retry on next launch).
     */
    fun clear(): Boolean {
        val ok = prefs?.edit()?.remove(KEY_TOKEN)?.commit() ?: false
        if (!ok) {
            Log.w(TAG, "clear() commit() returned false; token may still be on disk")
        }
        cached = null
        return ok
    }

    companion object {
        private const val TAG = "EncryptedTokenStore"
        private const val FILE = "axon_secrets"
        private const val KEY_TOKEN = "bearer_token"
    }
}
