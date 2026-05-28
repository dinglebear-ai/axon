package com.axon.app.ui.settings

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.ApiToken
import com.axon.app.data.repository.AxonSettings
import com.axon.app.data.repository.ServerUrl
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch

sealed interface TestConnectionState {
    data object Idle : TestConnectionState
    data object Testing : TestConnectionState
    /** Connection succeeded. [warning] is non-null when the URL uses cleartext HTTP. */
    data class Ok(val warning: String? = null) : TestConnectionState
    data class Failed(val error: String) : TestConnectionState
}

sealed interface SaveState {
    data object Idle : SaveState
    data object Saving : SaveState
    data object Saved : SaveState
    data class Failed(val error: String) : SaveState
}

class SettingsViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _settings = MutableStateFlow(AxonSettings())
    val settings: StateFlow<AxonSettings> = _settings.asStateFlow()

    private val _connection = MutableStateFlow<TestConnectionState>(TestConnectionState.Idle)
    val connection: StateFlow<TestConnectionState> = _connection.asStateFlow()

    private val _saveState = MutableStateFlow<SaveState>(SaveState.Idle)
    val saveState: StateFlow<SaveState> = _saveState.asStateFlow()

    init {
        container.settingsRepository.settings
            .onEach { _settings.value = it }
            .launchIn(viewModelScope)
    }

    fun saveSettings(serverUrl: String, token: String, collection: String) {
        val updated = AxonSettings(
            serverUrl  = ServerUrl(serverUrl.trim()),
            token      = ApiToken(token.trim()),
            collection = collection.trim(),
        )
        viewModelScope.launch {
            _saveState.value = SaveState.Saving
            runCatching {
                // (1) Persist to DataStore first
                container.settingsRepository.save(updated)
                // (2) Then update the shared client with the saved values
                container.axonClient.updateConfig(updated.serverUrl.value, updated.token.value)
            }.fold(
                onSuccess = { _saveState.value = SaveState.Saved },
                onFailure = { cause ->
                    // DataStore write failures (e.g. disk full, permissions) must be surfaced
                    // explicitly — silent failure here leaves the client with stale credentials
                    // and the user with no idea their settings were not persisted.
                    _saveState.value = SaveState.Failed(
                        cause.message ?: "Failed to save settings"
                    )
                },
            )
        }
    }

    fun testConnection(serverUrl: String, token: String) {
        viewModelScope.launch {
            _connection.value = TestConnectionState.Testing
            // Use a temporary throwaway client — do NOT mutate the shared client before saving
            val trimmedUrl = serverUrl.trim()
            val tempClient = AxonClient(trimmedUrl, token.trim())
            val result = tempClient.healthz()
            _connection.value = result.fold(
                onSuccess = {
                    val warning = if (trimmedUrl.startsWith("http://")) {
                        "Warning: cleartext HTTP is in use. Consider switching to HTTPS for non-Tailscale servers."
                    } else {
                        null
                    }
                    TestConnectionState.Ok(warning = warning)
                },
                onFailure = { cause ->
                    // Surface the actual failure cause so users know whether it's a 401,
                    // DNS failure, TLS error, or something else — not just "Server unreachable".
                    TestConnectionState.Failed(cause.message ?: "Server unreachable")
                },
            )
        }
    }
}
