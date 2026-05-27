package com.axon.app.ui.query

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

sealed interface QueryUiState {
    object Idle : QueryUiState
    object Loading : QueryUiState
    /** Server responded but returned no matching chunks. */
    object Empty : QueryUiState
    /** At least one result was returned. [hits] is guaranteed non-empty. */
    data class Results(val hits: List<QueryHitUi>) : QueryUiState
    data class Error(val message: String) : QueryUiState
}

class QueryViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _uiState = MutableStateFlow<QueryUiState>(QueryUiState.Idle)
    val uiState: StateFlow<QueryUiState> = _uiState.asStateFlow()

    fun query(query: String) {
        if (query.isBlank()) return
        viewModelScope.launch {
            _uiState.value = QueryUiState.Loading
            val collection = container.settingsRepository.settings.first().collection
            container.axonRepository.query(query, limit = 20, collection = collection).fold(
                onSuccess = { hits ->
                    _uiState.value = if (hits.isEmpty()) QueryUiState.Empty else QueryUiState.Results(hits)
                },
                onFailure = { err -> _uiState.value = QueryUiState.Error(err.message ?: "Error") },
            )
        }
    }
}
