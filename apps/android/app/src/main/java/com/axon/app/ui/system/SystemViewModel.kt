package com.axon.app.ui.system

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.ui.common.Resource
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonElement

private const val TAG = "SystemViewModel"

/**
 * System page ViewModel — Doctor only for Wave 2b.
 *
 * Fires `/v1/doctor` on init and exposes a [refresh] method for the manual
 * refresh button. State machine uses the shared [Resource] sealed interface
 * (R8). Stack/Config sub-sections are deferred to a later wave.
 */
class SystemViewModel(
    app: Application,
) : AndroidViewModel(app) {

    private val container = (app as AxonApp).container

    private val _doctor = MutableStateFlow<Resource<JsonElement>>(Resource.Loading)
    val doctor: StateFlow<Resource<JsonElement>> = _doctor.asStateFlow()

    init { refresh() }

    fun refresh() {
        viewModelScope.launch {
            _doctor.value = Resource.Loading
            container.axonRepository.doctorPayload().fold(
                onSuccess = { _doctor.value = Resource.Ready(it) },
                onFailure = {
                    Log.w(TAG, "doctor failed", it)
                    _doctor.value = Resource.Error(it.message ?: "Error")
                },
            )
        }
    }
}
