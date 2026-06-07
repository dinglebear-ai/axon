package com.axon.app.ui.setup

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.ui.common.Resource
import com.axon.app.ui.common.humanSummary
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class SetupViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _smokeState = MutableStateFlow<Resource<String>>(Resource.Idle)
    val smokeState: StateFlow<Resource<String>> = _smokeState.asStateFlow()

    private val _doctorState = MutableStateFlow<Resource<String>>(Resource.Idle)
    val doctorState: StateFlow<Resource<String>> = _doctorState.asStateFlow()

    fun runSmoke() {
        if (_smokeState.value is Resource.Loading) return
        viewModelScope.launch {
            _smokeState.value = Resource.Loading
            runCatching { container.axonClient.healthz() }
                .fold(
                    onSuccess = { result ->
                        result.fold(
                            onSuccess = { _smokeState.value = Resource.Ready("/healthz → 200 OK") },
                            onFailure = { e ->
                                val hint = e.message?.take(120) ?: e.javaClass.simpleName
                                _smokeState.value = Resource.Error("Unreachable: $hint")
                            },
                        )
                    },
                    onFailure = { e ->
                        val hint = e.message?.take(120) ?: e.javaClass.simpleName
                        _smokeState.value = Resource.Error("Unexpected error: $hint")
                    },
                )
        }
    }

    fun runDoctor() {
        if (_doctorState.value is Resource.Loading) return
        viewModelScope.launch {
            _doctorState.value = Resource.Loading
            runCatching { container.axonClient.doctor() }
                .fold(
                    onSuccess = { result ->
                        result.fold(
                            onSuccess = { resp ->
                                _doctorState.value = Resource.Ready(resp.payload.humanSummary())
                            },
                            onFailure = { e ->
                                val hint = e.message?.take(120) ?: e.javaClass.simpleName
                                _doctorState.value = Resource.Error("Doctor unavailable: $hint")
                            },
                        )
                    },
                    onFailure = { e ->
                        val hint = e.message?.take(120) ?: e.javaClass.simpleName
                        _doctorState.value = Resource.Error("Unexpected error: $hint")
                    },
                )
        }
    }
}
