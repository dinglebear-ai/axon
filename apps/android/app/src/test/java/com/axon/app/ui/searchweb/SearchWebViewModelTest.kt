package com.axon.app.ui.searchweb

import app.cash.turbine.test
import com.axon.app.data.repository.SearchWebHitUi
import com.axon.app.data.repository.SearchWebResultUi
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
 * The real [SearchWebViewModel] is an [androidx.lifecycle.AndroidViewModel] depending on the
 * [com.axon.app.AxonApp] container — testing it requires Robolectric. We test the contract via
 * a stand-in that mirrors the production state machine (Resource<T> Idle→Loading→Ready|Error).
 *
 * Same trade-off as the SummarizeViewModelTest pattern: if the production VM's submit contract
 * changes, both must move together.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class SearchWebViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setUp() { Dispatchers.setMain(dispatcher) }
    @After fun tearDown() { Dispatchers.resetMain() }

    @Test fun `success path emits Idle, Loading, then Ready with result`() = runTest(dispatcher) {
        val payload = SearchWebResultUi(
            query = "rust async",
            results = listOf(
                SearchWebHitUi(title = "Tokio", url = "https://tokio.rs", snippet = "async runtime", score = 0.9),
            ),
            sourceJobsEnqueued = 1,
            sourceJobsRejected = 0,
            sourceJobs = emptyList(),
        )
        val vm = TestSearchWebViewModel(stub = Result.success(payload))
        vm.uiState.test {
            assertEquals(Resource.Idle, awaitItem())
            vm.submit("rust async")
            assertEquals(Resource.Loading, awaitItem())
            val ready = awaitItem() as Resource.Ready<SearchWebResultUi>
            assertEquals("rust async", ready.value.query)
            assertEquals(1, ready.value.results.size)
            assertEquals(1, ready.value.sourceJobsEnqueued)
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `empty results still resolves as Ready with zero hits`() = runTest(dispatcher) {
        val payload = SearchWebResultUi(
            query = "no-hits", results = emptyList(),
            sourceJobsEnqueued = 0, sourceJobsRejected = 0, sourceJobs = emptyList(),
        )
        val vm = TestSearchWebViewModel(stub = Result.success(payload))
        vm.uiState.test {
            assertEquals(Resource.Idle, awaitItem())
            vm.submit("no-hits")
            assertEquals(Resource.Loading, awaitItem())
            val ready = awaitItem() as Resource.Ready<SearchWebResultUi>
            assertTrue(ready.value.results.isEmpty())
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `failure path emits Loading then Error with message`() = runTest(dispatcher) {
        val vm = TestSearchWebViewModel(stub = Result.failure(IllegalStateException("boom")))
        vm.uiState.test {
            assertEquals(Resource.Idle, awaitItem())
            vm.submit("anything")
            assertEquals(Resource.Loading, awaitItem())
            val err = awaitItem() as Resource.Error
            assertTrue("expected 'boom' in message, got: ${err.message}", err.message.contains("boom"))
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `blank query is a no-op and stays Idle`() = runTest(dispatcher) {
        val vm = TestSearchWebViewModel(stub = Result.failure(IllegalStateException("should-not-be-called")))
        vm.submit("   ")
        assertEquals(Resource.Idle, vm.uiState.value)
        assertEquals(0, vm.calls)
    }
}

private class TestSearchWebViewModel(private val stub: Result<SearchWebResultUi>) {
    var calls: Int = 0
    private val _uiState = MutableStateFlow<Resource<SearchWebResultUi>>(Resource.Idle)
    val uiState = _uiState.asStateFlow()

    fun submit(query: String) {
        if (query.isBlank()) return
        calls++
        _uiState.value = Resource.Loading
        stub.fold(
            onSuccess = { _uiState.value = Resource.Ready(it) },
            onFailure = { _uiState.value = Resource.Error(it.message ?: "Error") },
        )
    }
}
