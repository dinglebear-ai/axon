@file:Suppress("DEPRECATION")

package com.axon.app.data.security

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey

/**
 * Single creation point for encrypted SharedPreferences.
 *
 * AndroidX Security Crypto is deprecated upstream, but these stores already
 * contain user auth state on disk. Keeping construction centralized lets us
 * migrate to a direct Android Keystore store later without changing every
 * credential caller or accidentally changing file names today.
 */
object SecurePrefsFactory {
    fun create(context: Context, fileName: String): SharedPreferences {
        val appContext = context.applicationContext
        val masterKey = MasterKey.Builder(appContext)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()

        return EncryptedSharedPreferences.create(
            appContext,
            fileName,
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
        )
    }
}
