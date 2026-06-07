package com.axon.app.ui.jobs

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.RecentJob
import com.axon.app.data.repository.WatchUi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

private const val OVERVIEW_TAG = "JobsOverviewVM"
private const val OVERVIEW_POLL_MS = 30_000L
private val ACTIVE_STATUSES = setOf("pending", "running", "processing")

/** Lightweight job-overview ViewModel for the drawer. Polling is active only while visible. */
class JobsOverviewViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container
    private val repo = container.axonRepository

    private val _activeJobs = MutableStateFlow<List<JobUi>>(emptyList())
    val activeJobs: StateFlow<List<JobUi>> = _activeJobs.asStateFlow()

    private val _jobsByKind = MutableStateFlow<Map<AxonClient.JobKind, List<JobUi>>>(emptyMap())
    val jobsByKind: StateFlow<Map<AxonClient.JobKind, List<JobUi>>> = _jobsByKind.asStateFlow()

    val recentJobs: StateFlow<List<RecentJob>> = container.recentJobs.recent
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val _watches = MutableStateFlow<List<WatchUi>>(emptyList())
    val watches: StateFlow<List<WatchUi>> = _watches.asStateFlow()

    private val _errorMessage = MutableStateFlow<String?>(null)
    val errorMessage: StateFlow<String?> = _errorMessage.asStateFlow()

    private val refreshCoordinator = JobsOverviewRefreshCoordinator(viewModelScope)
    private val poller = JobsOverviewPoller(
        scope = viewModelScope,
        pollIntervalMs = OVERVIEW_POLL_MS,
        refresh = { refreshNow() },
    )

    fun setVisible(visible: Boolean) {
        poller.setVisible(visible)
    }

    fun refresh() {
        viewModelScope.launch { refreshNow() }
    }

    private suspend fun refreshNow() {
        refreshCoordinator.refresh {
            loadOverview()
        }
    }

    private suspend fun loadOverview() {
        val kinds = AxonClient.JobKind.entries
        val recent = container.recentJobs.recent.first()
        val all = mutableListOf<JobUi>()
        val byKind = mutableMapOf<AxonClient.JobKind, List<JobUi>>()
        var failures = 0
        var firstError: String? = null
        for (kind in kinds) {
            repo.listJobs(kind).fold(
                onSuccess = { jobs ->
                    byKind[kind] = jobs
                    all += jobs.filter { it.status in ACTIVE_STATUSES }
                },
                onFailure = { e ->
                    failures++
                    if (firstError == null) firstError = e.message
                    Log.w(OVERVIEW_TAG, "listJobs($kind) failed", e)
                    val fallback = recent
                        .filter { it.kind.equals(kind.path, ignoreCase = true) }
                        .map { it.toFallbackJob(kind) }
                    if (fallback.isNotEmpty()) {
                        byKind[kind] = fallback
                        all += fallback.filter { it.status in ACTIVE_STATUSES }
                    }
                },
            )
        }
        repo.listWatches().fold(
            onSuccess = { _watches.value = it },
            onFailure = { e ->
                Log.w(OVERVIEW_TAG, "listWatches failed", e)
            },
        )
        _jobsByKind.value = byKind
        _activeJobs.value = all
        _errorMessage.value = if (failures == kinds.size && byKind.isEmpty()) firstError else null
    }

    private fun RecentJob.toFallbackJob(kind: AxonClient.JobKind): JobUi =
        JobUi(
            kind = kind,
            id = jobId,
            status = "pending",
            url = target,
            sourceType = null,
            target = target,
            errorText = null,
            resultJson = null,
            finishedAt = null,
        )
}
