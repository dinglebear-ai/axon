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

data class ConfigFileUiState(
    val envValues: Map<String, String> = AxonSettingsCatalog.envDefaults,
    val configValues: Map<String, String> = AxonSettingsCatalog.configDefaults,
    val envExplicit: Set<String> = emptySet(),
    val configExplicit: Set<String> = emptySet(),
    val envPath: String = "~/.axon/.env",
    val configPath: String = "~/.axon/config.toml",
    val loading: Boolean = true,
    val error: String? = null,
)

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

    private val _configFiles = MutableStateFlow(ConfigFileUiState())
    val configFiles: StateFlow<ConfigFileUiState> = _configFiles.asStateFlow()

    init {
        container.settingsRepository.settings
            .onEach { _settings.value = it }
            .launchIn(viewModelScope)
        refreshConfigFiles()
    }

    fun saveAll(serverUrl: String, token: String, panelToken: String, collection: String) {
        val updated = AxonSettings(
            serverUrl  = ServerUrl(serverUrl.trim()),
            token      = ApiToken(token.trim()),
            panelToken = ApiToken(panelToken.trim()),
            collection = collection.trim(),
        )
        viewModelScope.launch {
            _saveState.value = SaveState.Saving
            runCatching {
                container.settingsRepository.save(updated)
                container.axonClient.updateConfig(updated.serverUrl.value, updated.token.value)
                container.axonClient.updatePanelToken(updated.panelToken.value)

                val filesToSave = if (_configFiles.value.error != null) {
                    loadConfigFilesFromServer()
                } else {
                    _configFiles.value
                }
                val rawEnv = renderEnvText(filesToSave.envValues)
                val rawToml = renderConfigTomlText(filesToSave.configValues)
                val envSave = container.axonClient.savePanelEnv(rawEnv)
                val configSave = container.axonClient.savePanelConfig(rawToml)
                if (!envSave.isSuccess || !configSave.isSuccess) {
                    throw envSave.exceptionOrNull()
                        ?: configSave.exceptionOrNull()
                        ?: IllegalStateException("Failed to save config files")
                }
            }.fold(
                onSuccess = {
                    _saveState.value = SaveState.Saved
                    refreshConfigFiles()
                },
                onFailure = { cause ->
                    _saveState.value = SaveState.Failed(
                        cause.message ?: "Failed to save settings"
                    )
                },
            )
        }
    }

    fun updateEnv(key: String, value: String) {
        val current = _configFiles.value
        _configFiles.value = current.copy(
            envValues = current.envValues + (key to value),
            envExplicit = if (value == AxonSettingsCatalog.envDefaults[key].orEmpty()) {
                current.envExplicit - key
            } else {
                current.envExplicit + key
            },
        )
    }

    fun updateConfig(key: String, value: String) {
        val current = _configFiles.value
        _configFiles.value = current.copy(
            configValues = current.configValues + (key to value),
            configExplicit = if (value == AxonSettingsCatalog.configDefaults[key].orEmpty()) {
                current.configExplicit - key
            } else {
                current.configExplicit + key
            },
        )
    }

    fun refreshConfigFiles() {
        viewModelScope.launch {
            _configFiles.value = _configFiles.value.copy(loading = true, error = null)
            runCatching { loadConfigFilesFromServer() }.fold(
                onSuccess = { _configFiles.value = it },
                onFailure = { cause ->
                    _configFiles.value = _configFiles.value.copy(
                        loading = false,
                        error = cause.message ?: "Could not load server config files",
                    )
                },
            )
        }
    }

    private suspend fun loadConfigFilesFromServer(): ConfigFileUiState {
        val envResult = container.axonClient.panelEnv()
        val configResult = container.axonClient.panelConfig()
        val env = envResult.getOrNull()
        val config = configResult.getOrNull()
        if (env == null || config == null) {
            throw IllegalStateException(
                envResult.exceptionOrNull()?.message
                    ?: configResult.exceptionOrNull()?.message
                    ?: "Could not load server config files",
            )
        }

        val explicitEnv = parseEnvText(env.rawEnv)
        val explicitConfig = parseConfigTomlText(config.rawToml)
        return ConfigFileUiState(
            envValues = AxonSettingsCatalog.envDefaults + explicitEnv,
            configValues = AxonSettingsCatalog.configDefaults + explicitConfig,
            envExplicit = explicitEnv.keys,
            configExplicit = explicitConfig.keys,
            envPath = env.path,
            configPath = config.path,
            loading = false,
        )
    }

    fun saveConfigFiles() {
        viewModelScope.launch {
            _saveState.value = SaveState.Saving
            runCatching {
                val current = if (_configFiles.value.error != null) {
                    loadConfigFilesFromServer()
                } else {
                    _configFiles.value
                }
                val rawEnv = renderEnvText(current.envValues)
                val rawToml = renderConfigTomlText(current.configValues)
                val envSave = container.axonClient.savePanelEnv(rawEnv)
                val configSave = container.axonClient.savePanelConfig(rawToml)
                if (!envSave.isSuccess || !configSave.isSuccess) {
                    throw envSave.exceptionOrNull()
                        ?: configSave.exceptionOrNull()
                        ?: IllegalStateException("Failed to save config files")
                }
            }.fold(
                onSuccess = {
                    _saveState.value = SaveState.Saved
                    refreshConfigFiles()
                },
                onFailure = { cause ->
                    _saveState.value = SaveState.Failed(
                        cause.message ?: "Failed to save config files",
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
