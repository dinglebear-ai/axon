package com.axon.app.ui.summarize

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.repository.SummarizeResultUi
import com.axon.app.data.util.UrlValidator
import com.axon.app.ui.common.Resource
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * Drives the Summarize mode screen. Single-URL synthesis via `/v1/summarize`,
 * which routes through the long-timeout OkHttp client because the server
 * invokes Gemini headlessly (can take a minute or more).
 *
 * State machine uses the shared [Resource] sealed interface (see R8 in the
 * Phase-2 plan revisions): Idle → Loading → Ready | Error.
 *
 * Invalid URLs (rejected by [UrlValidator.isValidHttpUrl]) are a no-op — we
 * stay in [Resource.Idle] and never round-trip to the server. This matches
 * the v1 plan's "invalid URL never calls the repository" test contract.
 */
class SummarizeViewModel(
    app: Application,
    @Suppress("unused") private val dispatcher: CoroutineDispatcher = Dispatchers.Main.immediate,
) : AndroidViewModel(app) {

    private val container = (app as AxonApp).container

    private val _uiState = MutableStateFlow<Resource<SummarizeResultUi>>(Resource.Idle)
    val uiState: StateFlow<Resource<SummarizeResultUi>> = _uiState.asStateFlow()

    fun submit(input: String) {
        if (!UrlValidator.isValidHttpUrl(input)) return
        viewModelScope.launch {
            _uiState.value = Resource.Loading
            // DataStore handles its own dispatcher; no need to wrap in withContext.
            val collection = container.settingsRepository.settings.first().collection
            container.axonRepository.summarize(listOf(input), collection).fold(
                onSuccess = { _uiState.value = Resource.Ready(it) },
                onFailure = { _uiState.value = Resource.Error(it.message ?: "Error") },
            )
        }
    }
}
