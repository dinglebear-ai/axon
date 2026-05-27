package com.axon.app.ui.operations

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Shared state for the Operations page: which mode the FAB has selected. The mode
 * persists across configuration changes and screen-recompositions, but resets when
 * the activity is destroyed (deliberately — there is no "last used mode" UX yet).
 */
class OperationsViewModel : ViewModel() {
    private val _activeMode = MutableStateFlow(OperationMode.Default)
    val activeMode: StateFlow<OperationMode> = _activeMode.asStateFlow()

    fun setMode(mode: OperationMode) {
        _activeMode.value = mode
    }
}
