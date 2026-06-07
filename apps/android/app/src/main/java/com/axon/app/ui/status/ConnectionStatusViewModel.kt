package com.axon.app.ui.status

import android.app.Application
import android.os.SystemClock
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

enum class ConnectionState { Checking, Online, Offline }

/** Health-check cadence. Server is LAN/Tailscale, not internet-scale. */
internal const val POLL_INTERVAL_MS = 30_000L

/**
 * Pure connection-state engine. Extracted from the AndroidViewModel so it can
 * be exercised under `runTest` with a virtual scheduler and a stubbed ping.
 *
 * The same `merge(poll + refresh) -> map -> catch -> stateIn` shape used by the
 * ViewModel; the only difference is `ping` is a function reference instead of
 * a hard call to `container.axonRepository.ping()`. Keep the call chain here
 * load-bearing — the ViewModel below is a thin shell around it.
 */
internal class ConnectionStatusEngine(
    private val ping: suspend () -> Boolean,
    private val pollIntervalMs: Long = POLL_INTERVAL_MS,
) {
    private val refreshTicker = Channel<Unit>(capacity = Channel.CONFLATED)

    fun state(scope: CoroutineScope): StateFlow<ConnectionState> = merge(
        flow {
            while (true) {
                emit(Unit)
                delay(pollIntervalMs)
            }
        },
        refreshTicker.receiveAsFlow(),
    )
        .map { _ ->
            // Handle exceptions inline so a single ping() failure doesn't terminate the
            // flow. The outer .catch is a safety net for unexpected framework exceptions.
            try {
                if (ping()) ConnectionState.Online else ConnectionState.Offline
            } catch (e: CancellationException) {
                throw e
            } catch (_: Throwable) {
                ConnectionState.Offline
            }
        }
        .catch { e ->
            if (e is CancellationException) throw e
            emit(ConnectionState.Offline)
        }
        .stateIn(scope, SharingStarted.WhileSubscribed(5_000), ConnectionState.Checking)

    fun refresh() {
        // Channel is CONFLATED — multiple rapid taps collapse into a single signal.
        refreshTicker.trySend(Unit)
    }
}

/**
 * Periodically pings `/healthz` and exposes the result as a [StateFlow].
 *
 * Polling is lifecycle-aware via `SharingStarted.WhileSubscribed(5_000)` — the
 * loop suspends 5 s after the last UI collector detaches and resumes on
 * resubscribe. No drain when the app is backgrounded.
 */
class ConnectionStatusViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container
    private val _latencyMs = MutableStateFlow<Long?>(null)

    private val engine = ConnectionStatusEngine(
        ping = {
            val started = SystemClock.elapsedRealtime()
            try {
                val ok = container.axonRepository.ping()
                _latencyMs.value = if (ok) SystemClock.elapsedRealtime() - started else null
                ok
            } catch (e: CancellationException) {
                throw e
            } catch (e: Throwable) {
                _latencyMs.value = null
                throw e
            }
        },
    )

    val state: StateFlow<ConnectionState> = engine.state(viewModelScope)
    val latencyMs: StateFlow<Long?> = _latencyMs.asStateFlow()

    fun refresh() {
        viewModelScope.launch { engine.refresh() }
    }
}
