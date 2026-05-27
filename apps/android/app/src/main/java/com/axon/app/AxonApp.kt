package com.axon.app

import android.app.Application
import com.axon.app.data.repository.DEFAULT_SERVER_URL
import com.axon.app.di.AppContainer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

class AxonApp : Application() {
    lateinit var container: AppContainer
        private set

    private val appScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
        appScope.launch {
            // If DataStore read fails (corrupted prefs, I/O error) we must still call
            // applySettings so isReady becomes true and the user can reach Settings to
            // reconfigure. Without this guard, isReady stays false forever and the app
            // shows a permanent spinner with no recovery path.
            val s = runCatching { container.settingsRepository.settings.first() }.getOrNull()
            if (s != null) {
                container.applySettings(s.serverUrl.value, s.token.value)
            } else {
                container.applySettings(DEFAULT_SERVER_URL, "")
            }
        }
    }
}
