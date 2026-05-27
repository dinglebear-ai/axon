package com.axon.app.data.repository

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map

// The "settings" DataStore is internal so the unit test in :app can inspect the
// same singleton instance the production code reads/writes. Marking it `private`
// would force tests to construct a second DataStore (different file lock) and the
// migration assertions would always pass against a fresh store.
internal val Context.settingsDataStore: DataStore<Preferences> by preferencesDataStore(name = "settings")

private val KEY_SERVER_URL  = stringPreferencesKey("server_url")
private val KEY_COLLECTION  = stringPreferencesKey("collection")
// KEY_TOKEN is no longer the source of truth for the token — kept only so the
// idempotent migration helper can find legacy plaintext copies and clear them.
internal val LEGACY_KEY_TOKEN: Preferences.Key<String> = stringPreferencesKey("token")

const val DEFAULT_SERVER_URL = "https://axon.tootie.tv"
const val DEFAULT_COLLECTION = "axon"

/** Wraps a server URL string. Prevents accidental use of a bare token or collection name as a URL. */
@JvmInline
value class ServerUrl(val value: String) {
    init { require(value.isNotBlank()) { "ServerUrl must not be blank" } }
    override fun toString(): String = value
}

/** Wraps a bearer token. Redacts the value from toString so it cannot be accidentally logged. */
@JvmInline
value class ApiToken(val value: String) {
    override fun toString(): String = if (value.isBlank()) "<no token>" else "ApiToken(***)"
    fun isBlank(): Boolean = value.isBlank()
}

data class AxonSettings(
    val serverUrl: ServerUrl = ServerUrl(DEFAULT_SERVER_URL),
    val token: ApiToken = ApiToken(""),
    val collection: String = DEFAULT_COLLECTION,
)

/**
 * SettingsRepository — server URL and collection live in plaintext DataStore;
 * the bearer token is delegated to [EncryptedTokenStore].
 *
 * To keep the [settings] flow reactive across token changes, this repository
 * mirrors the encrypted token through a [MutableStateFlow] kept in sync by
 * [save] / [clearToken]. Direct mutation of [EncryptedTokenStore] from other
 * call sites will not propagate to observers — go through this repository.
 */
class SettingsRepository(
    private val context: Context,
    private val encrypted: EncryptedTokenStore = EncryptedTokenStore(context),
) {
    // Seed the mirror with whatever the encrypted store currently has. Subsequent
    // writes via save()/clearToken() update both the store and this StateFlow.
    private val tokenMirror = MutableStateFlow(encrypted.read().orEmpty())

    val settings: Flow<AxonSettings> = context.settingsDataStore.data
        .map { prefs ->
            // Guard against a blank stored value (e.g. a DataStore entry written as "" before
            // validation was added). ServerUrl.init requires non-blank, so fall back to the
            // default at the call site rather than letting the value class throw.
            val rawUrl = prefs[KEY_SERVER_URL]?.takeIf { it.isNotBlank() } ?: DEFAULT_SERVER_URL
            val collection = prefs[KEY_COLLECTION] ?: DEFAULT_COLLECTION
            rawUrl to collection
        }
        .combine(tokenMirror) { (rawUrl, collection), token ->
            AxonSettings(
                serverUrl  = ServerUrl(rawUrl),
                token      = ApiToken(token),
                collection = collection,
            )
        }

    suspend fun save(settings: AxonSettings) {
        context.settingsDataStore.edit { prefs ->
            prefs[KEY_SERVER_URL]  = settings.serverUrl.value
            prefs[KEY_COLLECTION]  = settings.collection
            // Defensive: ensure any lingering legacy plaintext token entry is removed.
            prefs.remove(LEGACY_KEY_TOKEN)
        }
        if (settings.token.value.isBlank()) {
            encrypted.clear()
        } else {
            encrypted.write(settings.token.value)
        }
        tokenMirror.value = settings.token.value
    }

    suspend fun clearToken() {
        encrypted.clear()
        tokenMirror.value = ""
        context.settingsDataStore.edit { it.remove(LEGACY_KEY_TOKEN) }
    }

    /**
     * Idempotent boot-time migration. Safe to call on every app start.
     *
     *  1. If the encrypted store already has a token, the migration is done —
     *     defensively wipe any legacy plaintext copy and return.
     *  2. Otherwise, if the plaintext DataStore has a non-blank token, move it
     *     into the encrypted store and remove the plaintext entry.
     *  3. If neither has a token, no-op.
     */
    suspend fun migrateTokenToEncrypted() {
        if (encrypted.read() != null) {
            context.settingsDataStore.edit { it.remove(LEGACY_KEY_TOKEN) }
            tokenMirror.value = encrypted.read().orEmpty()
            return
        }
        val plain = context.settingsDataStore.data.first()[LEGACY_KEY_TOKEN]?.takeIf { it.isNotBlank() }
            ?: return
        encrypted.write(plain)
        context.settingsDataStore.edit { it.remove(LEGACY_KEY_TOKEN) }
        tokenMirror.value = plain
    }
}
