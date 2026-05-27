package com.axon.app.ui.document

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.repository.RetrieveResultUi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

private const val TAG = "DocumentViewModel"

sealed interface DocumentUiState {
    object Loading : DocumentUiState
    data class Success(val result: RetrieveResultUi) : DocumentUiState
    data class Error(val message: String) : DocumentUiState
}

/**
 * Loads the full assembled document for a URL from `/v1/retrieve`.
 *
 * URL is passed through [load], not the constructor, so the ViewModel can be
 * resolved via `viewModel()` without a `SavedStateHandle` factory. The
 * "already loaded" dedupe key is the URL of the **last successful** load —
 * failed loads are not memoised, so navigating back to a URL whose first
 * attempt errored will retry on next composition.
 */
class DocumentViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _uiState = MutableStateFlow<DocumentUiState>(DocumentUiState.Loading)
    val uiState: StateFlow<DocumentUiState> = _uiState.asStateFlow()

    /** URL of the most recent **successful** load. `null` while loading or after a failure. */
    private var lastLoadedUrl: String? = null

    fun load(url: String) {
        if (lastLoadedUrl == url) return
        viewModelScope.launch { fetch(url) }
    }

    /** Re-run the last load even when it matches the dedupe key. Used by the error-state retry button. */
    fun retry(url: String) {
        viewModelScope.launch { fetch(url) }
    }

    private suspend fun fetch(url: String) {
        _uiState.value = DocumentUiState.Loading
        val collection = container.settingsRepository.settings.first().collection
        container.axonRepository.retrieve(url, collection = collection).fold(
            onSuccess = {
                lastLoadedUrl = url
                _uiState.value = DocumentUiState.Success(it)
            },
            onFailure = { err ->
                lastLoadedUrl = null
                Log.w(TAG, "retrieve($url) failed", err)
                val kind = err::class.simpleName ?: "Error"
                _uiState.value = DocumentUiState.Error(
                    err.message?.let { "$kind: $it" } ?: kind,
                )
            },
        )
    }
}
