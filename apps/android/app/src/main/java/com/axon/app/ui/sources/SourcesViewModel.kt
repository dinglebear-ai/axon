package com.axon.app.ui.sources

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.repository.SourceEntryUi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

sealed interface SourcesUiState {
    object Loading : SourcesUiState
    /** Server responded but no sources are indexed yet. */
    object Empty : SourcesUiState
    /** At least one source is indexed. [sources] is guaranteed non-empty. */
    data class Loaded(val sources: List<SourceEntryUi>, val total: Int) : SourcesUiState
    data class Error(val message: String) : SourcesUiState
}

class SourcesViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _uiState = MutableStateFlow<SourcesUiState>(SourcesUiState.Loading)
    val uiState: StateFlow<SourcesUiState> = _uiState.asStateFlow()

    init { load() }

    fun load() {
        viewModelScope.launch {
            _uiState.value = SourcesUiState.Loading
            val collection = container.settingsRepository.settings.first().collection
            container.axonRepository.sources(limit = 100, collection = collection).fold(
                onSuccess = { list ->
                    _uiState.value = if (list.isEmpty()) {
                        SourcesUiState.Empty
                    } else {
                        SourcesUiState.Loaded(
                            sources = list,
                            total = list.sumOf { it.chunks },
                        )
                    }
                },
                onFailure = { err -> _uiState.value = SourcesUiState.Error(err.message ?: "Error") },
            )
        }
    }
}
