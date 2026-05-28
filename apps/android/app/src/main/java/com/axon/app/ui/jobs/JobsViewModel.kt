package com.axon.app.ui.jobs

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.RecentJob
import com.axon.app.ui.common.Resource
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonElement

private const val TAG = "JobsViewModel"

private const val POLL_INTERVAL_MS = 15_000L

/**
 * Drives the Jobs page.
 *
 * Polling model (R10): a single [_selectedTab] [MutableStateFlow] drives a
 * `flatMapLatest`-spawned poll flow per tab. Only the visible tab polls — when
 * the user switches tabs the previous tab's poll is auto-cancelled. This deletes
 * the 4× bandwidth/power burst of the v1 plan's per-kind parallel `stateIn` flows.
 *
 * Tab → JobKind mapping lives in [JobsScreen]'s `tabKinds` list (R15) — keep the
 * ordering authority there so reordering tabs doesn't desync the cancel target.
 *
 * Status header: one-shot fetch of `/v1/status` at construction time. Not polled
 * because the counts are derived and refresh naturally via the per-tab list polls.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class JobsViewModel(
    app: Application,
) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _selectedTab = MutableStateFlow(AxonClient.JobKind.Crawl)

    /** Visible-tab job list (R10). Uses capped exponential backoff on network failures. */
    val visibleJobs: StateFlow<Resource<List<JobUi>>> = _selectedTab
        .flatMapLatest { kind ->
            flow {
                var backoffMs = POLL_INTERVAL_MS
                while (true) {
                    val r = container.axonRepository.listJobs(kind)
                    backoffMs = if (r.isSuccess) POLL_INTERVAL_MS else (backoffMs * 2).coerceAtMost(120_000L)
                    emit(
                        r.fold(
                            onSuccess = { Resource.Ready(it) },
                            onFailure = { Resource.Error(it.message ?: "Error") },
                        )
                    )
                    delay(backoffMs)
                }
            }
        }
        .catch { e -> if (e is CancellationException) throw e; emit(Resource.Error(e.message ?: "Error")) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), Resource.Loading)

    private val _statusPayload = MutableStateFlow<JsonElement?>(null)
    val statusPayload: StateFlow<JsonElement?> = _statusPayload.asStateFlow()

    /** Persisted "Recent submissions" — survives process death. */
    val recent: StateFlow<List<RecentJob>> = container.recentJobs.recent
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * One-shot user-visible messages (cancel succeeded, cancel failed, status
     * fetch failed). Replay = 0 so each message fires once; buffer of 4 so a
     * burst of taps doesn't drop earlier events.
     */
    private val _messages = MutableSharedFlow<String>(replay = 0, extraBufferCapacity = 4, onBufferOverflow = BufferOverflow.DROP_OLDEST)
    val messages: SharedFlow<String> = _messages.asSharedFlow()

    init {
        viewModelScope.launch {
            container.axonRepository.statusPayload().fold(
                onSuccess = { _statusPayload.value = it },
                onFailure = {
                    Log.w(TAG, "status fetch failed", it)
                    _messages.tryEmit("Status header unavailable: ${it.message ?: "error"}")
                },
            )
        }
    }

    /** Switch the visible tab. Triggers `flatMapLatest` to cancel + restart the poll. */
    fun selectTab(kind: AxonClient.JobKind) {
        _selectedTab.value = kind
    }

    /**
     * Cancel the given `jobId` under the currently-selected tab.
     *
     * Reads the kind from [_selectedTab] (NOT from a caller-supplied tab index)
     * so the cancel always targets the kind the user is currently viewing —
     * avoiding the index-mismatch class of bugs R15 is guarding against.
     */
    fun cancel(jobId: String) {
        val kind = _selectedTab.value
        viewModelScope.launch {
            container.axonRepository.cancelJob(kind, jobId).fold(
                onSuccess = { canceled ->
                    if (!canceled) {
                        _messages.tryEmit("Server reports job $jobId was not cancelable.")
                    }
                    // Successful cancel — next poll surfaces the new status.
                },
                onFailure = {
                    Log.w(TAG, "cancelJob $kind/$jobId failed", it)
                    _messages.tryEmit("Failed to cancel ${kind.name.lowercase()} job: ${it.message ?: "error"}")
                },
            )
        }
    }
}
