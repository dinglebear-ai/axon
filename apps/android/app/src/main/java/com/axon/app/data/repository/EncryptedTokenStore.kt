package com.axon.app.data.repository

import android.content.Context
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
        }.getOrElse {
            // Master key invalidated or shared-prefs file corrupted (e.g. AEADBadTagException
            // raised during EncryptedSharedPreferences.create()). Delete the file and surface
            // null so callers force re-auth instead of crashing on every read.
            context.deleteSharedPreferences(FILE)
            null
        }
    }

    fun read(): String? {
        cached?.let { return it }
        val p = prefs ?: return null
        return runCatching { p.getString(KEY_TOKEN, null) }
            .getOrElse {
                // Master key invalidated at read time; clear and force re-auth.
                context.deleteSharedPreferences(FILE)
                cached = null
                null
            }
            .also { cached = it }
    }

    /** Synchronous commit — credentials must survive immediate process kill. */
    fun write(token: String) {
        val p = prefs ?: return
        @Suppress("ApplySharedPref")
        p.edit().putString(KEY_TOKEN, token).commit()
        cached = token
    }

    fun clear() {
        prefs?.edit()?.remove(KEY_TOKEN)?.commit()
        cached = null
    }

    companion object {
        private const val FILE = "axon_secrets"
        private const val KEY_TOKEN = "bearer_token"
    }
}
