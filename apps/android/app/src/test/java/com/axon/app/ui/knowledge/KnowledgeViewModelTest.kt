package com.axon.app.ui.knowledge

import app.cash.turbine.test
import com.axon.app.data.repository.DomainFacetUi
import com.axon.app.data.repository.SourceEntryUi
import com.axon.app.data.repository.SuggestHitUi
import com.axon.app.ui.common.Resource
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * The real [KnowledgeViewModel] is an [androidx.lifecycle.AndroidViewModel] that
 * depends on the [com.axon.app.AxonApp] container. We test the contract
 * (Resource<T> state machine + R11 30s memoization) via a stand-in that mirrors
 * the production logic. If the production VM's contract changes, both must move
 * together — same trade-off as [com.axon.app.ui.summarize.SummarizeViewModelTest].
 */
@OptIn(ExperimentalCoroutinesApi::class)
class KnowledgeViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setUp() { Dispatchers.setMain(dispatcher) }
    @After fun tearDown() { Dispatchers.resetMain() }

    @Test fun `suggest success emits Idle Loading Ready`() = runTest(dispatcher) {
        val vm = TestKnowledgeViewModel(
            suggestResult = Result.success(listOf(SuggestHitUi("https://a", "good"))),
        )
        vm.suggest.test {
            assertEquals(Resource.Idle, awaitItem())
            vm.loadSuggest(null)
            assertEquals(Resource.Loading, awaitItem())
            val ready = awaitItem() as Resource.Ready<List<SuggestHitUi>>
            assertEquals(1, ready.value.size)
            assertEquals("https://a", ready.value[0].url)
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `sources failure emits Error with message`() = runTest(dispatcher) {
        val vm = TestKnowledgeViewModel(
            sourcesResult = Result.failure(IllegalStateException("boom")),
        )
        vm.sources.test {
            assertEquals(Resource.Idle, awaitItem())
            vm.loadSources()
            assertEquals(Resource.Loading, awaitItem())
            val err = awaitItem() as Resource.Error
            assertTrue(err.message.contains("boom"))
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test fun `R11 memoization - second call within 30s returns cached Ready and skips fetch`() = runTest(dispatcher) {
        val vm = TestKnowledgeViewModel(
            domainsResult = Result.success(listOf(DomainFacetUi("example.com", 42))),
        )
        // First call: actually fires
        vm.loadDomains()
        assertEquals(1, vm.domainsCalls)
        // Stays Ready
        assertTrue(vm.domains.value is Resource.Ready<*>)

        // Second call within 30s window: skipped
        vm.advanceClock(10_000L)
        vm.loadDomains()
        assertEquals(1, vm.domainsCalls)

        // force=true bypasses memoization
        vm.loadDomains(force = true)
        assertEquals(2, vm.domainsCalls)
    }

    @Test fun `R11 memoization - call after 30s window triggers fresh fetch`() = runTest(dispatcher) {
        val vm = TestKnowledgeViewModel(
            statsResult = Result.success(JsonObject(mapOf("k" to JsonPrimitive("v")))),
        )
        vm.loadStats()
        assertEquals(1, vm.statsCalls)
        vm.advanceClock(31_000L)
        vm.loadStats()
        assertEquals(2, vm.statsCalls)
    }

    @Test fun `non forced load does not restart an in flight section`() = runTest(dispatcher) {
        val vm = TestKnowledgeViewModel(
            suggestResult = Result.success(listOf(SuggestHitUi("https://a", "good"))),
        )
        vm.markSuggestLoading()
        vm.loadSuggest(null)
        assertEquals(0, vm.suggestCalls)
        assertEquals(Resource.Loading, vm.suggest.value)
    }
}

/**
 * Mirrors [KnowledgeViewModel]'s state machine + R11 memoization contract.
 * Tracks per-section call counts so memoization tests can assert "no extra fetch".
 */
private class TestKnowledgeViewModel(
    private val suggestResult: Result<List<SuggestHitUi>> = Result.success(emptyList()),
    private val sourcesResult: Result<List<SourceEntryUi>> = Result.success(emptyList()),
    private val domainsResult: Result<List<DomainFacetUi>> = Result.success(emptyList()),
    private val statsResult: Result<kotlinx.serialization.json.JsonElement> = Result.success(JsonObject(emptyMap())),
) {
    private val _suggest = MutableStateFlow<Resource<List<SuggestHitUi>>>(Resource.Idle)
    val suggest = _suggest.asStateFlow()
    private val _sources = MutableStateFlow<Resource<List<SourceEntryUi>>>(Resource.Idle)
    val sources = _sources.asStateFlow()
    private val _domains = MutableStateFlow<Resource<List<DomainFacetUi>>>(Resource.Idle)
    val domains = _domains.asStateFlow()
    private val _stats = MutableStateFlow<Resource<kotlinx.serialization.json.JsonElement>>(Resource.Idle)
    val stats = _stats.asStateFlow()

    var suggestCalls = 0
    var domainsCalls = 0
    var statsCalls = 0

    private var clockMs: Long = 0L
    private var suggestCachedAt: Long? = null
    private var sourcesCachedAt: Long? = null
    private var domainsCachedAt: Long? = null
    private var statsCachedAt: Long? = null

    fun advanceClock(deltaMs: Long) { clockMs += deltaMs }
    private fun now(): Long = clockMs
    private fun fresh(at: Long?) = at != null && (now() - at) < 30_000L
    fun markSuggestLoading() { _suggest.value = Resource.Loading }

    fun loadSuggest(focus: String?, force: Boolean = false) {
        if (!force && fresh(suggestCachedAt) && _suggest.value is Resource.Ready<*>) return
        if (!force && _suggest.value is Resource.Loading) return
        suggestCalls++
        _suggest.value = Resource.Loading
        suggestResult.fold(
            onSuccess = { _suggest.value = Resource.Ready(it); suggestCachedAt = now() },
            onFailure = { _suggest.value = Resource.Error(it.message ?: "Error"); suggestCachedAt = null },
        )
    }

    fun loadSources(force: Boolean = false) {
        if (!force && fresh(sourcesCachedAt) && _sources.value is Resource.Ready<*>) return
        if (!force && _sources.value is Resource.Loading) return
        _sources.value = Resource.Loading
        sourcesResult.fold(
            onSuccess = { _sources.value = Resource.Ready(it); sourcesCachedAt = now() },
            onFailure = { _sources.value = Resource.Error(it.message ?: "Error"); sourcesCachedAt = null },
        )
    }

    fun loadDomains(force: Boolean = false) {
        if (!force && fresh(domainsCachedAt) && _domains.value is Resource.Ready<*>) return
        if (!force && _domains.value is Resource.Loading) return
        domainsCalls++
        _domains.value = Resource.Loading
        domainsResult.fold(
            onSuccess = { _domains.value = Resource.Ready(it); domainsCachedAt = now() },
            onFailure = { _domains.value = Resource.Error(it.message ?: "Error"); domainsCachedAt = null },
        )
    }

    fun loadStats(force: Boolean = false) {
        if (!force && fresh(statsCachedAt) && _stats.value is Resource.Ready<*>) return
        if (!force && _stats.value is Resource.Loading) return
        statsCalls++
        _stats.value = Resource.Loading
        statsResult.fold(
            onSuccess = { _stats.value = Resource.Ready(it); statsCachedAt = now() },
            onFailure = { _stats.value = Resource.Error(it.message ?: "Error"); statsCachedAt = null },
        )
    }
}
