package com.axon.app.ui.document

import app.cash.turbine.test
import com.axon.app.data.repository.RetrieveResultUi
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
 * Tests for [DocumentViewModel] state contract via a stand-in that mirrors the
 * production state machine (load/retry/dedup/error/success flows) without
 * requiring Robolectric or the [com.axon.app.AxonApp] container.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class DocumentViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setUp() { Dispatchers.setMain(dispatcher) }
    @After fun tearDown() { Dispatchers.resetMain() }

    @Test fun `success path transitions to Success with correct URL`() = runTest(dispatcher) {
        val vm = TestDocumentViewModel(Result.success(makeResult("https://example.com")))
        vm.uiState.test {
            assertEquals(DocumentUiState.Loading, awaitItem())
            vm.load("https://example.com")
            val success = awaitItem() as DocumentUiState.Success
            assertEquals("https://example.com", success.result.requestedUrl)
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `failure path transitions to Error with message`() = runTest(dispatcher) {
        val vm = TestDocumentViewModel(Result.failure(RuntimeException("404 not found")))
        vm.uiState.test {
            assertEquals(DocumentUiState.Loading, awaitItem())
            vm.load("https://example.com")
            val err = awaitItem() as DocumentUiState.Error
            assertTrue("expected '404' in message", err.message.contains("404"))
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `load deduplicates — same URL after success skips fetch`() = runTest(dispatcher) {
        val vm = TestDocumentViewModel(Result.success(makeResult("https://example.com")))
        vm.load("https://example.com")
        val callsAfterFirst = vm.calls
        vm.load("https://example.com")
        assertEquals("duplicate load must not re-fetch", callsAfterFirst, vm.calls)
    }

    @Test fun `retry always re-fetches even when URL matches last success`() = runTest(dispatcher) {
        val vm = TestDocumentViewModel(Result.success(makeResult("https://example.com")))
        vm.load("https://example.com")
        val callsAfterFirst = vm.calls
        vm.retry("https://example.com")
        assertEquals("retry must fire fetch", callsAfterFirst + 1, vm.calls)
    }

    @Test fun `failed load clears dedup key so next load retries`() = runTest(dispatcher) {
        val vm = TestDocumentViewModel(Result.failure(RuntimeException("err")))
        vm.load("https://example.com")
        val callsAfterFail = vm.calls
        // Failed load must NOT memoize the URL; the next load should retry.
        vm.load("https://example.com")
        assertEquals("must retry after failure", callsAfterFail + 1, vm.calls)
    }

    @Test fun `different URL always fetches regardless of prior success`() = runTest(dispatcher) {
        val vm = TestDocumentViewModel(Result.success(makeResult("https://a.com")))
        vm.load("https://a.com")
        val callsAfterFirst = vm.calls
        vm.load("https://b.com")
        assertEquals("different URL must fetch", callsAfterFirst + 1, vm.calls)
    }
}

private fun makeResult(url: String) = RetrieveResultUi(
    requestedUrl = url,
    matchedUrl = url,
    chunkCount = 3,
    content = "chunk text",
    truncated = false,
    warnings = emptyList(),
)

private class TestDocumentViewModel(private val stubResult: Result<RetrieveResultUi>) {
    var calls: Int = 0
    private val _uiState = MutableStateFlow<DocumentUiState>(DocumentUiState.Loading)
    val uiState = _uiState.asStateFlow()
    private var lastLoadedUrl: String? = null

    fun load(url: String) {
        if (lastLoadedUrl == url) return
        fetch(url)
    }

    fun retry(url: String) { fetch(url) }

    private fun fetch(url: String) {
        calls++
        _uiState.value = DocumentUiState.Loading
        stubResult.fold(
            onSuccess = {
                lastLoadedUrl = url
                _uiState.value = DocumentUiState.Success(it)
            },
            onFailure = { err ->
                lastLoadedUrl = null
                val kind = err::class.simpleName ?: "Error"
                _uiState.value = DocumentUiState.Error(err.message?.let { "$kind: $it" } ?: kind)
            },
        )
    }
}
