package com.axon.app.ui.settings

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.AxonSettings
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch

sealed interface ConnectionState {
    object Idle : ConnectionState
    object Testing : ConnectionState
    /** Connection succeeded. [warning] is non-null when the URL uses cleartext HTTP. */
    data class Ok(val warning: String? = null) : ConnectionState
    data class Failed(val error: String) : ConnectionState
}

class SettingsViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _settings = MutableStateFlow(AxonSettings())
    val settings: StateFlow<AxonSettings> = _settings.asStateFlow()

    private val _connection = MutableStateFlow<ConnectionState>(ConnectionState.Idle)
    val connection: StateFlow<ConnectionState> = _connection.asStateFlow()

    init {
        container.settingsRepository.settings
            .onEach { _settings.value = it }
            .launchIn(viewModelScope)
    }

    fun saveSettings(serverUrl: String, token: String, collection: String) {
        val updated = AxonSettings(serverUrl = serverUrl.trim(), token = token.trim(), collection = collection.trim())
        viewModelScope.launch {
            // (1) Persist to DataStore first
            container.settingsRepository.save(updated)
            // (2) Then update the shared client with the saved values
            container.axonClient.updateConfig(updated.serverUrl, updated.token)
        }
    }

    fun testConnection(serverUrl: String, token: String) {
        viewModelScope.launch {
            _connection.value = ConnectionState.Testing
            // Use a temporary throwaway client — do NOT mutate the shared client before saving
            val trimmedUrl = serverUrl.trim()
            val tempClient = AxonClient(trimmedUrl, token.trim())
            val ok = tempClient.healthz()
            _connection.value = if (ok) {
                val warning = if (trimmedUrl.startsWith("http://")) {
                    "Warning: cleartext HTTP is in use. Consider switching to HTTPS for non-Tailscale servers."
                } else {
                    null
                }
                ConnectionState.Ok(warning = warning)
            } else {
                ConnectionState.Failed("Server unreachable")
            }
        }
    }
}
