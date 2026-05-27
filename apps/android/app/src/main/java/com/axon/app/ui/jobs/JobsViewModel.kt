package com.axon.app.ui.jobs

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.RecentJob
import com.axon.app.ui.common.Resource
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonElement

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
    @Suppress("unused") private val dispatcher: CoroutineDispatcher = Dispatchers.Main.immediate,
) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _selectedTab = MutableStateFlow(AxonClient.JobKind.Crawl)

    /** Visible-tab job list (R10). */
    val visibleJobs: StateFlow<Resource<List<JobUi>>> = _selectedTab
        .flatMapLatest { kind ->
            flow {
                while (true) {
                    val r = container.axonRepository.listJobs(kind)
                    emit(
                        r.fold(
                            onSuccess = { Resource.Ready(it) },
                            onFailure = { Resource.Error(it.message ?: "Error") },
                        )
                    )
                    delay(POLL_INTERVAL_MS)
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

    init {
        viewModelScope.launch {
            container.axonRepository.statusPayload().onSuccess { _statusPayload.value = it }
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
            container.axonRepository.cancelJob(kind, jobId)
            // The next poll cycle will surface the updated status; no explicit refresh needed.
        }
    }
}
