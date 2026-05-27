package com.axon.app.ui.status

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

enum class ConnectionState { Checking, Online, Offline }

/** Health-check cadence. Server is LAN/Tailscale, not internet-scale. */
private const val POLL_INTERVAL_MS = 30_000L

/**
 * Periodically pings `/healthz` and exposes the result as a [StateFlow].
 *
 * Polling is lifecycle-aware via `SharingStarted.WhileSubscribed(5_000)` — the
 * loop suspends 5 s after the last UI collector detaches and resumes on
 * resubscribe. No drain when the app is backgrounded.
 */
class ConnectionStatusViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    /** Conflated refresh signal — tapping the indicator merges into the poll flow. */
    private val refreshTicker = Channel<Unit>(capacity = Channel.CONFLATED)

    val state: StateFlow<ConnectionState> = merge(
        flow {
            while (true) {
                emit(Unit)
                delay(POLL_INTERVAL_MS)
            }
        },
        refreshTicker.receiveAsFlow(),
    )
        .map {
            if (container.axonRepository.ping()) ConnectionState.Online else ConnectionState.Offline
        }
        .catch { e ->
            if (e is CancellationException) throw e
            emit(ConnectionState.Offline)
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ConnectionState.Checking)

    fun refresh() {
        viewModelScope.launch { refreshTicker.send(Unit) }
    }
}
