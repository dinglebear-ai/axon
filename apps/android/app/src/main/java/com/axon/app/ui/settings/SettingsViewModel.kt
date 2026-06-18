package com.axon.app.ui.settings

import android.app.Application
import android.content.Intent
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.auth.AuthMode
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
import java.net.URI

internal const val REDACTED_SECRET_VALUE = "••••••••"

private val CLEARTEXT_TAILNET_SUFFIXES = setOf(
    "manatee-triceratops.ts.net",
    "manatee-triceratops.tailvpn.net",
)

internal fun validateAxonServerUrl(value: String) {
    val uri = runCatching { URI(value) }.getOrNull()
    val scheme = uri?.scheme?.lowercase()
    val host = uri?.host?.lowercase()
    if (value.isBlank() || host.isNullOrBlank() || (scheme != "http" && scheme != "https")) {
        throw IllegalArgumentException("Server URL must start with http:// or https://")
    }
    if (scheme == "http" && CLEARTEXT_TAILNET_SUFFIXES.none { host == it || host.endsWith(".$it") }) {
        throw IllegalArgumentException("Use HTTPS for non-Tailscale servers. Cleartext HTTP is allowed only for configured tailnet domains.")
    }
}

internal fun redactConfigValuesForUi(
    values: Map<String, String>,
    secretKeys: Set<String>,
): Map<String, String> =
    values.mapValues { (key, value) ->
        if (key in secretKeys && value.isNotBlank()) REDACTED_SECRET_VALUE else value
    }

internal fun dirtyKeysForSecretSafeSave(
    values: Map<String, String>,
    dirtyKeys: Set<String>,
    secretKeys: Set<String>,
): Set<String> =
    dirtyKeys.filterNot { key ->
        key in secretKeys && values[key] == REDACTED_SECRET_VALUE
    }.toSet()

data class ConfigFileUiState(
    val envValues: Map<String, String> = AxonSettingsCatalog.envDefaults,
    val configValues: Map<String, String> = AxonSettingsCatalog.configDefaults,
    val envExplicit: Set<String> = emptySet(),
    val configExplicit: Set<String> = emptySet(),
    val envDirty: Set<String> = emptySet(),
    val configDirty: Set<String> = emptySet(),
    val rawEnv: String = "",
    val rawConfig: String = "",
    val envPath: String = "~/.axon/.env",
    val configPath: String = "~/.axon/config.toml",
    val loading: Boolean = true,
    val error: String? = null,
)

data class CollectionListUiState(
    val collections: List<String> = emptyList(),
    val loading: Boolean = false,
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

enum class OAuthUiStatus { Idle, Starting, SignedIn, Error }

class SettingsViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container
    private var serverRawEnv: String = ""
    private var serverRawConfig: String = ""
    private var pendingOAuthServerUrl: String? = null

    private val _settings = MutableStateFlow(AxonSettings())
    val settings: StateFlow<AxonSettings> = _settings.asStateFlow()

    private val _connection = MutableStateFlow<TestConnectionState>(TestConnectionState.Idle)
    val connection: StateFlow<TestConnectionState> = _connection.asStateFlow()

    private val _saveState = MutableStateFlow<SaveState>(SaveState.Idle)
    val saveState: StateFlow<SaveState> = _saveState.asStateFlow()

    private val _draftAuthMode = MutableStateFlow(AuthMode.Bearer)
    val draftAuthMode: StateFlow<AuthMode> = _draftAuthMode.asStateFlow()

    private val _oauthStatus = MutableStateFlow(OAuthUiStatus.Idle)
    val oauthStatus: StateFlow<OAuthUiStatus> = _oauthStatus.asStateFlow()

    private val _configFiles = MutableStateFlow(ConfigFileUiState())
    val configFiles: StateFlow<ConfigFileUiState> = _configFiles.asStateFlow()

    private val _collections = MutableStateFlow(CollectionListUiState())
    val collections: StateFlow<CollectionListUiState> = _collections.asStateFlow()

    init {
        container.settingsRepository.settings
            .onEach {
                _settings.value = it
                _draftAuthMode.value = it.authMode
                _oauthStatus.value = if (it.authMode == AuthMode.OAuth && container.oauthRepository.isSignedIn()) {
                    OAuthUiStatus.SignedIn
                } else {
                    OAuthUiStatus.Idle
                }
            }
            .launchIn(viewModelScope)
        refreshConfigFiles()
        refreshCollections()
    }

    fun saveConnection(serverUrl: String, token: String, collection: String) {
        viewModelScope.launch {
            _saveState.value = SaveState.Saving
            runCatching {
                val trimmedServerUrl = serverUrl.trim()
                val trimmedToken = token.trim()
                validateAxonServerUrl(trimmedServerUrl)
                val updated = AxonSettings(
                    serverUrl = ServerUrl(trimmedServerUrl),
                    token = ApiToken(trimmedToken),
                    collection = collection.trim(),
                    authMode = when (_draftAuthMode.value) {
                        AuthMode.OAuth -> {
                            if (!container.oauthRepository.isSignedIn()) {
                                throw IllegalStateException("Sign in with OAuth before saving OAuth mode.")
                            }
                            AuthMode.OAuth
                        }
                        AuthMode.Bearer -> AuthMode.Bearer
                    },
                )
                container.settingsRepository.save(updated)
                container.applySettings(updated.serverUrl.value, updated.token.value, updated.authMode)
            }.fold(
                onSuccess = {
                    _saveState.value = SaveState.Saved
                    refreshConfigFiles()
                    refreshCollections()
                },
                onFailure = { cause ->
                    _saveState.value = SaveState.Failed(
                        cause.message ?: "Failed to save settings"
                    )
                },
            )
        }
    }

    fun setDraftAuthMode(mode: AuthMode) {
        _draftAuthMode.value = mode
    }

    suspend fun beginOAuthSignIn(serverUrl: String): Result<Intent> = runCatching {
        if (_oauthStatus.value == OAuthUiStatus.Starting) error("OAuth sign-in already in progress")
        _oauthStatus.value = OAuthUiStatus.Starting
        val trimmedServerUrl = serverUrl.trim()
        validateAxonServerUrl(trimmedServerUrl)
        pendingOAuthServerUrl = trimmedServerUrl.trimEnd('/')
        container.oauthRepository.createAuthorizationRequest(trimmedServerUrl)
    }.onFailure {
        pendingOAuthServerUrl = null
        _oauthStatus.value = OAuthUiStatus.Error
        _saveState.value = SaveState.Failed(it.message ?: "OAuth sign-in failed")
    }

    fun completeOAuthSignIn(intent: Intent?) {
        viewModelScope.launch {
            if (intent == null) {
                cancelOAuthSignInInternal()
                return@launch
            }
            container.oauthRepository.handleAuthorizationResponse(intent).fold(
                onSuccess = {
                    val signedInServerUrl = pendingOAuthServerUrl
                        ?: _settings.value.serverUrl.value.trim().trimEnd('/')
                    val updated = _settings.value.copy(
                        serverUrl = ServerUrl(signedInServerUrl),
                        authMode = AuthMode.OAuth,
                    )
                    container.settingsRepository.save(updated)
                    container.applySettings(updated.serverUrl.value, updated.token.value, updated.authMode)
                    _settings.value = updated
                    _draftAuthMode.value = AuthMode.OAuth
                    _oauthStatus.value = OAuthUiStatus.SignedIn
                    _saveState.value = SaveState.Saved
                    pendingOAuthServerUrl = null
                },
                onFailure = { cause ->
                    pendingOAuthServerUrl = null
                    _oauthStatus.value = OAuthUiStatus.Error
                    _saveState.value = SaveState.Failed(cause.message ?: "OAuth sign-in failed")
                },
            )
        }
    }

    fun cancelOAuthSignIn() {
        viewModelScope.launch {
            cancelOAuthSignInInternal()
        }
    }

    fun signOutOAuth() {
        viewModelScope.launch {
            val cleared = container.oauthRepository.signOut()
            if (!cleared) {
                _saveState.value = SaveState.Failed("Could not clear OAuth credentials")
                return@launch
            }
            val updated = _settings.value.copy(authMode = AuthMode.Bearer)
            container.settingsRepository.save(updated)
            container.applySettings(updated.serverUrl.value, updated.token.value, updated.authMode)
            _settings.value = updated
            _draftAuthMode.value = AuthMode.Bearer
            _oauthStatus.value = OAuthUiStatus.Idle
            _saveState.value = SaveState.Saved
        }
    }

    fun updateEnv(key: String, value: String) {
        val current = _configFiles.value
        _configFiles.value = current.copy(
            envValues = current.envValues + (key to value),
            envDirty = current.envDirty + key,
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
            configDirty = current.configDirty + key,
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

    fun refreshCollections() {
        viewModelScope.launch {
            _collections.value = _collections.value.copy(loading = true, error = null)
            container.axonClient.collections().recoverCatching {
                container.axonClient.panelCollections().getOrThrow()
            }.fold(
                onSuccess = { response ->
                    _collections.value = CollectionListUiState(
                        collections = response.collections.distinct().sorted(),
                    )
                },
                onFailure = { cause ->
                    _collections.value = CollectionListUiState(
                        error = cause.message ?: "Could not load Qdrant collections",
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
        serverRawEnv = env.rawEnv
        serverRawConfig = config.rawToml
        return ConfigFileUiState(
            envValues = AxonSettingsCatalog.envDefaults + redactConfigValuesForUi(
                explicitEnv,
                AxonSettingsCatalog.envSecretKeys,
            ),
            configValues = AxonSettingsCatalog.configDefaults + redactConfigValuesForUi(
                explicitConfig,
                AxonSettingsCatalog.configSecretKeys,
            ),
            envExplicit = explicitEnv.keys,
            configExplicit = explicitConfig.keys,
            rawEnv = redactEnvText(env.rawEnv, AxonSettingsCatalog.envSecretKeys),
            rawConfig = redactConfigTomlText(config.rawToml, AxonSettingsCatalog.configSecretKeys),
            envPath = env.path,
            configPath = config.path,
            loading = false,
        )
    }

    fun saveEnvFile() {
        viewModelScope.launch {
            _saveState.value = SaveState.Saving
            runCatching {
                val current = configFilesReadyForSave()
                val dirtyKeys = dirtyKeysForSecretSafeSave(
                    current.envValues,
                    current.envDirty,
                    AxonSettingsCatalog.envSecretKeys,
                )
                val latestEnv = latestRawEnvForSave(current.rawEnv)
                val rawEnv = patchEnvText(latestEnv, current.envValues, dirtyKeys)
                val envSave = container.axonClient.savePanelEnv(rawEnv)
                if (!envSave.isSuccess) {
                    throw envSave.exceptionOrNull()
                        ?: IllegalStateException("Failed to save .env")
                }
                _configFiles.value = current.copy(
                    rawEnv = redactEnvText(rawEnv, AxonSettingsCatalog.envSecretKeys),
                    envDirty = emptySet(),
                    envExplicit = parseEnvText(rawEnv).keys,
                )
                serverRawEnv = rawEnv
            }.fold(
                onSuccess = {
                    _saveState.value = SaveState.Saved
                },
                onFailure = { cause ->
                    _saveState.value = SaveState.Failed(
                        cause.message ?: "Failed to save .env",
                    )
                },
            )
        }
    }

    fun saveConfigFile() {
        viewModelScope.launch {
            _saveState.value = SaveState.Saving
            runCatching {
                val current = configFilesReadyForSave()
                val dirtyKeys = dirtyKeysForSecretSafeSave(
                    current.configValues,
                    current.configDirty,
                    AxonSettingsCatalog.configSecretKeys,
                )
                val latestConfig = latestRawConfigForSave(current.rawConfig)
                val rawToml = patchConfigTomlText(latestConfig, current.configValues, dirtyKeys)
                val configSave = container.axonClient.savePanelConfig(rawToml)
                if (!configSave.isSuccess) {
                    throw configSave.exceptionOrNull()
                        ?: IllegalStateException("Failed to save config.toml")
                }
                _configFiles.value = current.copy(
                    rawConfig = redactConfigTomlText(rawToml, AxonSettingsCatalog.configSecretKeys),
                    configDirty = emptySet(),
                    configExplicit = parseConfigTomlText(rawToml).keys,
                )
                serverRawConfig = rawToml
            }.fold(
                onSuccess = {
                    _saveState.value = SaveState.Saved
                },
                onFailure = { cause ->
                    _saveState.value = SaveState.Failed(
                        cause.message ?: "Failed to save config.toml",
                    )
                },
            )
        }
    }

    private fun configFilesReadyForSave(): ConfigFileUiState {
        val current = _configFiles.value
        if (current.loading) {
            throw IllegalStateException("Config files are still loading")
        }
        current.error?.let { throw IllegalStateException(it) }
        return current
    }

    private suspend fun cancelOAuthSignInInternal() {
        container.oauthRepository.cancelSignIn()
        pendingOAuthServerUrl = null
        _oauthStatus.value = OAuthUiStatus.Error
        _saveState.value = SaveState.Failed("OAuth sign-in was cancelled")
    }

    private suspend fun latestRawEnvForSave(fallbackRawEnv: String): String =
        container.axonClient.panelEnv()
            .getOrNull()
            ?.rawEnv
            ?: serverRawEnv.ifBlank { fallbackRawEnv }

    private suspend fun latestRawConfigForSave(fallbackRawConfig: String): String =
        container.axonClient.panelConfig()
            .getOrNull()
            ?.rawToml
            ?: serverRawConfig.ifBlank { fallbackRawConfig }

    fun testConnection(serverUrl: String, token: String) {
        viewModelScope.launch {
            _connection.value = TestConnectionState.Testing
            // Use a temporary throwaway client — do NOT mutate the shared client before saving
            val trimmedUrl = serverUrl.trim()
            runCatching { validateAxonServerUrl(trimmedUrl) }.onFailure { cause ->
                _connection.value = TestConnectionState.Failed(cause.message ?: "Invalid server URL")
                return@launch
            }
            val tempClient = AxonClient(trimmedUrl, token.trim())
            val result = tempClient.healthz()
            _connection.value = result.fold(
                onSuccess = {
                    val warning = if (trimmedUrl.startsWith("http://")) {
                        "Warning: cleartext HTTP is allowed here only because this host matches the Tailscale allowlist."
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
