package com.axon.app.ui.status

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * Connection state surfaced by [ConnectionStatusIndicator]. Drives both the
 * `AuroraStatusTone` and the human-readable label in the top app bar.
 */
enum class ConnectionState { Checking, Online, Offline }

/** Health-check cadence. Kept conservative — server is local-network or LAN, not internet-scale. */
private const val POLL_INTERVAL_MS = 30_000L

/**
 * Periodically pings `/healthz` so the top app bar can show whether the Axon
 * server is reachable. Polling runs inside `viewModelScope` and stops with the
 * ViewModel — there is no lifecycle leak across activity destroy.
 */
class ConnectionStatusViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _state = MutableStateFlow(ConnectionState.Checking)
    val state: StateFlow<ConnectionState> = _state.asStateFlow()

    init {
        viewModelScope.launch {
            while (isActive) {
                _state.value = if (container.axonRepository.ping()) {
                    ConnectionState.Online
                } else {
                    ConnectionState.Offline
                }
                delay(POLL_INTERVAL_MS)
            }
        }
    }

    /** Trigger an out-of-band check (e.g. when the user taps the indicator). */
    fun refresh() {
        viewModelScope.launch {
            _state.value = ConnectionState.Checking
            _state.value = if (container.axonRepository.ping()) {
                ConnectionState.Online
            } else {
                ConnectionState.Offline
            }
        }
    }
}
