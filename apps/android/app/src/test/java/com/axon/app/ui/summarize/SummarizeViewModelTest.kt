package com.axon.app.ui.summarize

import app.cash.turbine.test
import com.axon.app.data.repository.SummarizeResultUi
import com.axon.app.data.util.UrlValidator
import com.axon.app.ui.common.Resource
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * The real [SummarizeViewModel] is an [androidx.lifecycle.AndroidViewModel] that
 * depends on the [com.axon.app.AxonApp] container — instantiating it requires a
 * Robolectric Application. We test the contract instead through a stand-in that
 * mirrors the production state machine (URL validation gate + Resource<T> states).
 *
 * The stand-in is intentionally a near-copy of [SummarizeViewModel.submit]'s body
 * minus AndroidViewModel plumbing; if the production VM's state contract changes,
 * both must be updated together — this is the simplicity vs. test-rig trade-off
 * captured in the plan (Step 3.1).
 */
@OptIn(ExperimentalCoroutinesApi::class)
class SummarizeViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setUp() { Dispatchers.setMain(dispatcher) }
    @After fun tearDown() { Dispatchers.resetMain() }

    @Test fun `success path emits Idle, Loading, then Ready with summary`() = runTest(dispatcher) {
        val vm = TestSummarizeViewModel(stubResult = Result.success(
            SummarizeResultUi(urls = listOf("https://a"), summary = "ok", contextChars = 7, contextTruncated = false)
        ))
        vm.uiState.test {
            assertEquals(Resource.Idle, awaitItem())
            vm.submit("https://a")
            assertEquals(Resource.Loading, awaitItem())
            val ready = awaitItem() as Resource.Ready<SummarizeResultUi>
            assertEquals("ok", ready.value.summary)
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `invalid URL never calls the repository and stays Idle`() = runTest(dispatcher) {
        val vm = TestSummarizeViewModel(stubResult = Result.success(
            SummarizeResultUi(emptyList(), "", 0, false)
        ))
        vm.submit("not-a-url")
        // Stays Idle, no Loading state, no repo call
        assertEquals(Resource.Idle, vm.uiState.value)
        assertEquals("expected zero repo calls", 0, vm.calls)
    }

    @Test fun `failure path emits Loading then Error with message`() = runTest(dispatcher) {
        val vm = TestSummarizeViewModel(stubResult = Result.failure(IllegalStateException("boom")))
        vm.uiState.test {
            assertEquals(Resource.Idle, awaitItem())
            vm.submit("https://example.com")
            assertEquals(Resource.Loading, awaitItem())
            val err = awaitItem() as Resource.Error
            assertTrue("expected 'boom' in message, got: ${err.message}", err.message.contains("boom"))
            cancelAndIgnoreRemainingEvents()
        }
    }
}

private class TestSummarizeViewModel(private val stubResult: Result<SummarizeResultUi>) {
    var calls: Int = 0
    private val _uiState = MutableStateFlow<Resource<SummarizeResultUi>>(Resource.Idle)
    val uiState = _uiState.asStateFlow()

    fun submit(input: String) {
        if (!UrlValidator.isValidHttpUrl(input)) return
        calls++
        _uiState.value = Resource.Loading
        stubResult.fold(
            onSuccess = { _uiState.value = Resource.Ready(it) },
            onFailure = { _uiState.value = Resource.Error(it.message ?: "Error") },
        )
    }
}
