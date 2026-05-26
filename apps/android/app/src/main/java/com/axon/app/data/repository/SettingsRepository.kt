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

data class AxonSettings(
    val serverUrl: String = DEFAULT_SERVER_URL,
    val token: String = "",
    val collection: String = DEFAULT_COLLECTION,
)

class SettingsRepository(private val context: Context) {

    val settings: Flow<AxonSettings> = context.dataStore.data.map { prefs ->
        AxonSettings(
            serverUrl  = prefs[KEY_SERVER_URL]  ?: DEFAULT_SERVER_URL,
            token      = prefs[KEY_TOKEN]       ?: "",
            collection = prefs[KEY_COLLECTION]  ?: DEFAULT_COLLECTION,
        )
    }

    suspend fun save(settings: AxonSettings) {
        context.dataStore.edit { prefs ->
            prefs[KEY_SERVER_URL]  = settings.serverUrl
            prefs[KEY_TOKEN]       = settings.token
            prefs[KEY_COLLECTION]  = settings.collection
        }
    }
}
