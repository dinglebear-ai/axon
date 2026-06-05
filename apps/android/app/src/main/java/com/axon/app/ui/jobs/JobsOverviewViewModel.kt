package com.axon.app.ui.jobs

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.WatchUi
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

private const val OVERVIEW_TAG = "JobsOverviewVM"
private const val OVERVIEW_POLL_MS = 30_000L
private val ACTIVE_STATUSES = setOf("pending", "running", "processing")

/** Lightweight job-overview ViewModel for the drawer. Polls all four kinds every 30s. */
class JobsOverviewViewModel(app: Application) : AndroidViewModel(app) {
    private val repo = (app as AxonApp).container.axonRepository

    private val _activeJobs = MutableStateFlow<List<JobUi>>(emptyList())
    val activeJobs: StateFlow<List<JobUi>> = _activeJobs.asStateFlow()

    private val _watches = MutableStateFlow<List<WatchUi>>(emptyList())
    val watches: StateFlow<List<WatchUi>> = _watches.asStateFlow()

    private val _errorMessage = MutableStateFlow<String?>(null)
    val errorMessage: StateFlow<String?> = _errorMessage.asStateFlow()

    private val refreshCoordinator = JobsOverviewRefreshCoordinator(viewModelScope)

    init { startPolling() }

    private fun startPolling() {
        viewModelScope.launch {
            while (true) {
                refreshNow()
                delay(OVERVIEW_POLL_MS)
            }
        }
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
        val all = mutableListOf<JobUi>()
        var failures = 0
        var firstError: String? = null
        for (kind in kinds) {
            repo.listJobs(kind).fold(
                onSuccess = { jobs -> all += jobs.filter { it.status in ACTIVE_STATUSES } },
                onFailure = { e ->
                    failures++
                    if (firstError == null) firstError = e.message
                    Log.w(OVERVIEW_TAG, "listJobs($kind) failed", e)
                },
            )
        }
        repo.listWatches().fold(
            onSuccess = { _watches.value = it },
            onFailure = { e ->
                Log.w(OVERVIEW_TAG, "listWatches failed", e)
            },
        )
        _activeJobs.value = all
        _errorMessage.value = if (failures == kinds.size && all.isEmpty()) firstError else null
    }
}
