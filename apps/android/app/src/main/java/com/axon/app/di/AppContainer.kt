package com.axon.app.di

import android.content.Context
import com.axon.app.data.local.AppDatabase
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.AxonRepository
import com.axon.app.data.repository.DEFAULT_SERVER_URL
import com.axon.app.data.repository.SettingsRepository

class AppContainer(context: Context) {
    val settingsRepository = SettingsRepository(context)
    private val db = AppDatabase.build(context)
    val askHistoryDao = db.askHistoryDao()

    val axonClient = AxonClient(
        baseUrl = DEFAULT_SERVER_URL,
        token = "",
    )

    val axonRepository = AxonRepository(axonClient)

    // Called once at app start after settings are read from DataStore
    fun applySettings(serverUrl: String, token: String) {
        axonClient.updateConfig(serverUrl, token)
    }
}
