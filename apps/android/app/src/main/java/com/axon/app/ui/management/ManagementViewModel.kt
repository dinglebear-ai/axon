package com.axon.app.ui.management

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.ui.common.Resource
import com.axon.app.ui.common.doctorServiceSummary
import com.axon.app.ui.common.humanSummary
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class ManagementViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _statsState = MutableStateFlow<Resource<String>>(Resource.Idle)
    val statsState: StateFlow<Resource<String>> = _statsState.asStateFlow()

    private val _doctorState = MutableStateFlow<Resource<String>>(Resource.Idle)
    val doctorState: StateFlow<Resource<String>> = _doctorState.asStateFlow()

    fun loadStats() {
        if (_statsState.value is Resource.Loading) return
        viewModelScope.launch {
            _statsState.value = Resource.Loading
            runCatching { container.axonClient.stats() }
                .fold(
                    onSuccess = { result ->
                        result.fold(
                            onSuccess = { resp ->
                                _statsState.value = Resource.Ready(resp.payload.humanSummary().ifBlank { "No collection data returned" })
                            },
                            onFailure = { e ->
                                val hint = e.message?.take(120) ?: e.javaClass.simpleName
                                _statsState.value = Resource.Error("Stats unavailable: $hint")
                            },
                        )
                    },
                    onFailure = { e ->
                        val hint = e.message?.take(120) ?: e.javaClass.simpleName
                        _statsState.value = Resource.Error("Unexpected error: $hint")
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
                                _doctorState.value = Resource.Ready(resp.payload.doctorServiceSummary())
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
