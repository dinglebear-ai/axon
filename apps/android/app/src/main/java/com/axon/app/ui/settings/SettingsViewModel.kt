package com.axon.app.ui.settings

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
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
    object Ok : ConnectionState
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
            container.settingsRepository.save(updated)
            container.axonClient.updateConfig(updated.serverUrl, updated.token)
        }
    }

    fun testConnection(serverUrl: String, token: String) {
        viewModelScope.launch {
            _connection.value = ConnectionState.Testing
            container.axonClient.updateConfig(serverUrl.trim(), token.trim())
            val ok = container.axonClient.healthz()
            _connection.value = if (ok) ConnectionState.Ok else ConnectionState.Failed("Server unreachable")
        }
    }
}
