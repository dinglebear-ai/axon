package com.axon.app.ui.search

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.repository.QueryHitUi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

sealed interface SearchUiState {
    object Idle : SearchUiState
    object Loading : SearchUiState
    /** Server responded but returned no matching chunks for this query. */
    object Empty : SearchUiState
    /** At least one result was returned. [hits] is guaranteed non-empty. */
    data class Results(val hits: List<QueryHitUi>) : SearchUiState
    data class Error(val message: String) : SearchUiState
}

class SearchViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _uiState = MutableStateFlow<SearchUiState>(SearchUiState.Idle)
    val uiState: StateFlow<SearchUiState> = _uiState.asStateFlow()

    fun search(query: String) {
        if (query.isBlank()) return
        viewModelScope.launch {
            _uiState.value = SearchUiState.Loading
            val collection = container.settingsRepository.settings.first().collection
            container.axonRepository.query(query, limit = 20, collection = collection).fold(
                onSuccess = { hits ->
                    _uiState.value = if (hits.isEmpty()) SearchUiState.Empty else SearchUiState.Results(hits)
                },
                onFailure = { err -> _uiState.value = SearchUiState.Error(err.message ?: "Error") },
            )
        }
    }
}
