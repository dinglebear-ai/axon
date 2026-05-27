package com.axon.app.di

import android.content.Context
import com.axon.app.data.local.AppDatabase
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.AxonRepository
import com.axon.app.data.repository.DEFAULT_SERVER_URL
import com.axon.app.data.repository.SettingsRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

// TODO: SECURITY - migrate token storage to EncryptedSharedPreferences before production use
// See: https://developer.android.com/reference/androidx/security/crypto/EncryptedSharedPreferences
class AppContainer(context: Context) {
    val settingsRepository = SettingsRepository(context)
    private val db = AppDatabase.build(context)

    val axonClient = AxonClient(
        baseUrl = DEFAULT_SERVER_URL,
        token = "",
    )

    val axonRepository = AxonRepository(
        client = axonClient,
        askHistoryDao = db.askHistoryDao(),
    )

    private val _isReady = MutableStateFlow(false)

    /** Becomes true once the initial DataStore settings have been applied to the client. */
    val isReady: StateFlow<Boolean> = _isReady.asStateFlow()

    // Called once at app start after settings are read from DataStore
    fun applySettings(serverUrl: String, token: String) {
        axonClient.updateConfig(serverUrl.trimEnd('/'), token)
        _isReady.value = true
    }
}
