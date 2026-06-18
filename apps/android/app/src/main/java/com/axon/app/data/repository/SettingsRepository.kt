package com.axon.app.data.repository

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.axon.app.data.auth.AuthMode
import com.axon.app.data.auth.authModeFromWireValue
import com.axon.app.data.auth.toWireValue
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
private val KEY_AUTH_MODE = stringPreferencesKey("auth_mode")
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
    val authMode: AuthMode = AuthMode.Bearer,
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
            val authMode = authModeFromWireValue(prefs[KEY_AUTH_MODE])
            Triple(rawUrl, collection, authMode)
        }
        .combine(tokenMirror) { (rawUrl, collection, authMode), token ->
            AxonSettings(
                serverUrl  = ServerUrl(rawUrl),
                token      = ApiToken(token),
                collection = collection,
                authMode   = authMode,
            )
        }

    /**
     * Persist [settings]. Throws [IllegalStateException] if the encrypted token
     * write fails so callers don't proceed under the illusion that credentials
     * were stored — the user can be re-prompted instead of silently bouncing
     * to a 401 wall on the next request.
     */
    suspend fun save(settings: AxonSettings) {
        context.settingsDataStore.edit { prefs ->
            prefs[KEY_SERVER_URL]  = settings.serverUrl.value
            prefs[KEY_COLLECTION]  = settings.collection
            prefs[KEY_AUTH_MODE] = settings.authMode.toWireValue()
            // Defensive: ensure any lingering legacy plaintext token entry is removed.
            prefs.remove(LEGACY_KEY_TOKEN)
        }
        val tokenOk = if (settings.token.value.isBlank()) {
            encrypted.clear()
        } else {
            encrypted.write(settings.token.value)
        }
        if (!tokenOk) {
            android.util.Log.w("SettingsRepository", "token store write failed; UI mirror NOT updated")
            throw IllegalStateException("Could not securely store credentials. Please try again.")
        }
        tokenMirror.value = settings.token.value
    }

    suspend fun clearToken() {
        val ok = encrypted.clear()
        if (!ok) {
            android.util.Log.w("SettingsRepository", "encrypted token clear() returned false; token may still be on disk")
        }
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
     *
     * Crash safety: the plaintext key is removed inside a `try { write } finally`
     * block — even if [encrypted.write] throws, or the process is killed between
     * the encrypted write and the plaintext removal, the next launch's
     * idempotency guard (`encrypted.read() != null`) wipes the legacy key.
     *
     * If [encrypted.write] returns false (encrypted prefs unavailable), the
     * plaintext entry is NOT removed — leaving it on disk is the correct
     * behaviour because the user would otherwise be silently logged out.
     */
    suspend fun migrateTokenToEncrypted() {
        if (encrypted.read() != null) {
            context.settingsDataStore.edit { it.remove(LEGACY_KEY_TOKEN) }
            tokenMirror.value = encrypted.read().orEmpty()
            return
        }
        val plain = context.settingsDataStore.data.first()[LEGACY_KEY_TOKEN]?.takeIf { it.isNotBlank() }
            ?: return
        val written = try {
            encrypted.write(plain)
        } catch (t: Throwable) {
            android.util.Log.w("SettingsRepository", "encrypted write failed during migration", t)
            false
        }
        if (!written) {
            // Encrypted prefs are unavailable. Leave the plaintext token in
            // place rather than silently logging the user out — the next launch
            // will retry the migration.
            return
        }
        // Encrypted write succeeded — guarantee the plaintext removal even if a
        // subsequent step throws.
        try {
            context.settingsDataStore.edit { it.remove(LEGACY_KEY_TOKEN) }
        } finally {
            tokenMirror.value = plain
        }
    }
}
