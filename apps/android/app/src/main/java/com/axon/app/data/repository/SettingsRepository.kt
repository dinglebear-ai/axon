package com.axon.app.data.repository

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "settings")

private val KEY_SERVER_URL  = stringPreferencesKey("server_url")
private val KEY_TOKEN       = stringPreferencesKey("token")
private val KEY_COLLECTION  = stringPreferencesKey("collection")

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

class SettingsRepository(private val context: Context) {

    val settings: Flow<AxonSettings> = context.dataStore.data.map { prefs ->
        // Guard against a blank stored value (e.g. a DataStore entry written as "" before
        // validation was added). ServerUrl.init requires non-blank, so fall back to the default
        // at the call site rather than letting the value class throw.
        val rawUrl = prefs[KEY_SERVER_URL]?.takeIf { it.isNotBlank() } ?: DEFAULT_SERVER_URL
        AxonSettings(
            serverUrl  = ServerUrl(rawUrl),
            token      = ApiToken(prefs[KEY_TOKEN]        ?: ""),
            collection = prefs[KEY_COLLECTION]            ?: DEFAULT_COLLECTION,
        )
    }

    suspend fun save(settings: AxonSettings) {
        context.dataStore.edit { prefs ->
            prefs[KEY_SERVER_URL]  = settings.serverUrl.value
            prefs[KEY_TOKEN]       = settings.token.value
            prefs[KEY_COLLECTION]  = settings.collection
        }
    }
}
