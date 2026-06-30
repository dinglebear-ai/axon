package com.axon.app.di

import android.content.Context
import com.axon.app.data.auth.AuthConfig
import com.axon.app.data.auth.AuthMode
import com.axon.app.data.auth.OAuthRepository
import com.axon.app.data.auth.OAuthStateStore
import com.axon.app.data.auth.OAuthTokenSource
import com.axon.app.data.local.AppDatabase
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.AxonRepository
import com.axon.app.data.repository.DEFAULT_SERVER_URL
import com.axon.app.data.repository.EncryptedHeadersStore
import com.axon.app.data.repository.EncryptedTokenStore
import com.axon.app.data.repository.ModeOptionsApplicator
import com.axon.app.data.repository.ModeOptionsRepository
import com.axon.app.data.repository.RecentJobsRepository
import com.axon.app.data.repository.SettingsRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Manual dependency-injection container for the application.
 *
 * Manual DI (rather than Hilt) is appropriate here because the graph is small and stable.
 * If the app grows to require scoped components or code-generated factories, migrate to Hilt.
 *
 * Lifecycle:
 * 1. [AppContainer] is created synchronously in [AxonApp.onCreate].
 * 2. [axonClient] is constructed with an empty bearer token — auth is unusable until step 3.
 * 3. The Application reads the first DataStore emission and calls [applySettings], which
 *    pushes real credentials into the client and sets [isReady] = true.
 * 4. The splash/gate composable observes [isReady] and blocks the UI until step 3 completes,
 *    ensuring no API calls reach the server with stale (empty) credentials.
 */
class AppContainer(context: Context) {
    val encryptedTokenStore = EncryptedTokenStore(context)
    val encryptedHeadersStore = EncryptedHeadersStore(context)
    val settingsRepository = SettingsRepository(context, encryptedTokenStore)
    val recentJobs = RecentJobsRepository(context)
    val modeOptionsRepository = ModeOptionsRepository(context, encryptedHeadersStore)
    val modeOptionsApplicator: ModeOptionsApplicator = modeOptionsRepository
    val database = AppDatabase.build(context)
    private val oauthStateStore by lazy { OAuthStateStore(context) }
    val oauthRepository by lazy { OAuthRepository(context, oauthStateStore) }
    private val oauthTokenSource = object : OAuthTokenSource {
        override suspend fun freshAccessToken(): Result<String> = oauthRepository.freshAccessToken()
        override fun isSignedIn(): Boolean = oauthRepository.isSignedIn()
    }

    val axonClient = AxonClient(
        baseUrl = DEFAULT_SERVER_URL,
        // Empty token on construction — real token applied in applySettings() before isReady fires.
        token = "",
    )

    val axonRepository = AxonRepository(
        client = axonClient,
        askHistoryDao = database.askHistoryDao(),
        applicator = modeOptionsApplicator,
    )

    private val _isReady = MutableStateFlow(false)

    /** Becomes true once the initial DataStore settings have been applied to the client. */
    val isReady: StateFlow<Boolean> = _isReady.asStateFlow()

    /** Called once at app start after the first DataStore settings emission is read. */
    fun applySettings(serverUrl: String, token: String, authMode: AuthMode) {
        val normalizedServerUrl = serverUrl.trimEnd('/')
        val auth = when (authMode) {
            AuthMode.Bearer -> AuthConfig.Bearer(token)
            AuthMode.OAuth -> AuthConfig.OAuth(oauthTokenSource, normalizedServerUrl)
        }
        axonClient.updateConfig(normalizedServerUrl, auth)
        _isReady.value = true
    }
}
