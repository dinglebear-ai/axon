package com.axon.app.ui.management

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

sealed interface MgmtActionState {
    data object Idle : MgmtActionState
    data object Loading : MgmtActionState
    data class Done(val summary: String) : MgmtActionState
    data class Error(val message: String) : MgmtActionState
}

class ManagementViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _statsState = MutableStateFlow<MgmtActionState>(MgmtActionState.Idle)
    val statsState: StateFlow<MgmtActionState> = _statsState.asStateFlow()

    private val _doctorState = MutableStateFlow<MgmtActionState>(MgmtActionState.Idle)
    val doctorState: StateFlow<MgmtActionState> = _doctorState.asStateFlow()

    fun loadStats() {
        if (_statsState.value is MgmtActionState.Loading) return
        viewModelScope.launch {
            _statsState.value = MgmtActionState.Loading
            container.axonClient.stats().fold(
                onSuccess = { resp ->
                    // payload is opaque JsonObject — show top-level key count as a summary
                    val preview = resp.payload.entries
                        .take(4)
                        .joinToString(" · ") { (k, v) -> "$k: $v" }
                    _statsState.value = MgmtActionState.Done(preview.ifBlank { "ok" })
                },
                onFailure = { e ->
                    _statsState.value = MgmtActionState.Error(e.message ?: "Stats unavailable")
                },
            )
        }
    }

    fun runDoctor() {
        if (_doctorState.value is MgmtActionState.Loading) return
        viewModelScope.launch {
            _doctorState.value = MgmtActionState.Loading
            container.axonClient.doctor().fold(
                onSuccess = { resp ->
                    val preview = resp.payload.toString().take(200)
                    _doctorState.value = MgmtActionState.Done(preview)
                },
                onFailure = { e ->
                    _doctorState.value = MgmtActionState.Error(e.message ?: "Doctor unavailable")
                },
            )
        }
    }
}
