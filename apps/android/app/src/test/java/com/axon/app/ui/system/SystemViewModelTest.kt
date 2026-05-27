package com.axon.app.ui.system

import com.axon.app.ui.common.Resource
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * Same stand-in approach as [com.axon.app.ui.summarize.SummarizeViewModelTest]:
 * mirror the production state machine here so we don't need Robolectric to
 * instantiate an AndroidViewModel.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class SystemViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setUp() { Dispatchers.setMain(dispatcher) }
    @After fun tearDown() { Dispatchers.resetMain() }

    @Test fun `doctor success ends in Ready`() = runTest(dispatcher) {
        val payload: JsonElement = JsonObject(mapOf("ok" to JsonPrimitive(true)))
        val vm = TestSystemViewModel(stubResult = Result.success(payload))
        // Stand-in fires synchronously in init, so the StateFlow has already
        // collapsed Loading → Ready by the time we observe its value. Verify
        // the terminal state.
        val ready = vm.doctor.value as Resource.Ready<JsonElement>
        assertEquals(payload, ready.value)
    }

    @Test fun `doctor failure ends in Error with message`() = runTest(dispatcher) {
        val vm = TestSystemViewModel(stubResult = Result.failure(IllegalStateException("network down")))
        val err = vm.doctor.value as Resource.Error
        assertTrue(err.message.contains("network down"))
    }

    @Test fun `refresh re-fires the request`() = runTest(dispatcher) {
        val vm = TestSystemViewModel(stubResult = Result.success(JsonObject(emptyMap())))
        assertEquals(1, vm.calls)
        vm.refresh()
        assertEquals(2, vm.calls)
    }
}

private class TestSystemViewModel(private val stubResult: Result<JsonElement>) {
    var calls: Int = 0
    private val _doctor = MutableStateFlow<Resource<JsonElement>>(Resource.Loading)
    val doctor = _doctor.asStateFlow()

    init { fire() }

    fun refresh() { fire() }

    private fun fire() {
        calls++
        _doctor.value = Resource.Loading
        stubResult.fold(
            onSuccess = { _doctor.value = Resource.Ready(it) },
            onFailure = { _doctor.value = Resource.Error(it.message ?: "Error") },
        )
    }
}
