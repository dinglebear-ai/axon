package com.axon.app.ui.setup

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

sealed interface SetupActionState {
    data object Idle : SetupActionState
    data object Running : SetupActionState
    data class Pass(val detail: String) : SetupActionState
    data class Fail(val message: String) : SetupActionState
}

class SetupViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _smokeState = MutableStateFlow<SetupActionState>(SetupActionState.Idle)
    val smokeState: StateFlow<SetupActionState> = _smokeState.asStateFlow()

    private val _doctorState = MutableStateFlow<SetupActionState>(SetupActionState.Idle)
    val doctorState: StateFlow<SetupActionState> = _doctorState.asStateFlow()

    fun runSmoke() {
        if (_smokeState.value is SetupActionState.Running) return
        viewModelScope.launch {
            _smokeState.value = SetupActionState.Running
            container.axonClient.healthz().fold(
                onSuccess = { _smokeState.value = SetupActionState.Pass("/healthz → 200 OK") },
                onFailure = { e -> _smokeState.value = SetupActionState.Fail(e.message ?: "Unreachable") },
            )
        }
    }

    fun runDoctor() {
        if (_doctorState.value is SetupActionState.Running) return
        viewModelScope.launch {
            _doctorState.value = SetupActionState.Running
            container.axonClient.doctor().fold(
                onSuccess = { resp ->
                    val preview = resp.payload.toString().take(300).trimEnd(',')
                    _doctorState.value = SetupActionState.Pass(preview)
                },
                onFailure = { e ->
                    _doctorState.value = SetupActionState.Fail(e.message ?: "Doctor unavailable")
                },
            )
        }
    }
}
