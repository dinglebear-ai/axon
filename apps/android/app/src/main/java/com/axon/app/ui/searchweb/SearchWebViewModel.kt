package com.axon.app.ui.searchweb

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.repository.SearchWebResultUi
import com.axon.app.ui.common.Resource
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * Drives the Search mode screen. Tavily-backed live web search via `/v1/search` —
 * the server auto-enqueues crawl jobs for results (subject to the
 * `AXON_MAX_PENDING_CRAWL_JOBS` cap; see R16 for the queue-full callout).
 *
 * State machine uses the shared [Resource] sealed interface (R8): Idle → Loading → Ready | Error.
 * Blank queries are a no-op and never round-trip to the server.
 */
class SearchWebViewModel(
    app: Application,
    @Suppress("unused") private val dispatcher: CoroutineDispatcher = Dispatchers.Main.immediate,
) : AndroidViewModel(app) {

    private val container = (app as AxonApp).container

    private val _uiState = MutableStateFlow<Resource<SearchWebResultUi>>(Resource.Idle)
    val uiState: StateFlow<Resource<SearchWebResultUi>> = _uiState.asStateFlow()

    fun submit(query: String) {
        val trimmed = query.trim()
        if (trimmed.isEmpty()) return
        viewModelScope.launch {
            _uiState.value = Resource.Loading
            container.axonRepository.searchWeb(trimmed).fold(
                onSuccess = { _uiState.value = Resource.Ready(it) },
                onFailure = { _uiState.value = Resource.Error(it.message ?: "Error") },
            )
        }
    }
}
