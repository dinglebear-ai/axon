package com.axon.app.ui.document

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.repository.RetrieveResultUi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

sealed interface DocumentUiState {
    object Loading : DocumentUiState
    data class Success(val result: RetrieveResultUi) : DocumentUiState
    data class Error(val message: String) : DocumentUiState
}

/**
 * Loads the full assembled document for a URL from `/v1/retrieve`. The URL is
 * passed via [load] (called once by the screen on first composition) rather
 * than the constructor so the ViewModel survives recomposition without a
 * SavedStateHandle factory.
 */
class DocumentViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _uiState = MutableStateFlow<DocumentUiState>(DocumentUiState.Loading)
    val uiState: StateFlow<DocumentUiState> = _uiState.asStateFlow()

    private var loaded: String? = null

    fun load(url: String) {
        if (loaded == url) return
        loaded = url
        viewModelScope.launch {
            _uiState.value = DocumentUiState.Loading
            val collection = container.settingsRepository.settings.first().collection
            container.axonRepository.retrieve(url, collection = collection).fold(
                onSuccess = { _uiState.value = DocumentUiState.Success(it) },
                onFailure = { _uiState.value = DocumentUiState.Error(it.message ?: "Error") },
            )
        }
    }
}
