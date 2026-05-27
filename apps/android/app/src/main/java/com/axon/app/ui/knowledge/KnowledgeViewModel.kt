package com.axon.app.ui.knowledge

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.repository.DomainFacetUi
import com.axon.app.data.repository.SourceEntryUi
import com.axon.app.data.repository.SuggestHitUi
import com.axon.app.ui.common.Resource
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonElement

private const val TAG = "KnowledgeViewModel"
private const val FRESHNESS_MS = 30_000L

/**
 * Knowledge page ViewModel.
 *
 * Four independent sections — each backed by a [Resource]<T> StateFlow (R8):
 * - **Suggest** (`/v1/suggest`) — `List<SuggestHitUi>`
 * - **Sources** (`/v1/sources`) — `List<SourceEntryUi>`
 * - **Domains** (`/v1/domains`) — `List<DomainFacetUi>`
 * - **Stats**   (`/v1/stats`)   — raw `JsonElement` (rendered chunked, R4)
 *
 * **R11 — 30s per-section memoization.** Each `loadX()` records `cachedAt`
 * after a successful fetch and short-circuits subsequent calls within 30s.
 * Failures clear the timestamp so the next visit retries. Pass `force=true`
 * to bypass (e.g. for a manual pull-to-refresh).
 *
 * Suggest is *not* memoized by focus value — calling `loadSuggest("a")` after
 * `loadSuggest("b")` within 30s will short-circuit on the prior `b` result.
 * That's intentional: the user explicitly hits Send to issue a new query, so
 * the section composable passes `force = true` for user-initiated submits.
 */
class KnowledgeViewModel(
    app: Application,
    @Suppress("unused") private val dispatcher: CoroutineDispatcher = Dispatchers.Main.immediate,
) : AndroidViewModel(app) {

    private val container = (app as AxonApp).container

    private val _suggest = MutableStateFlow<Resource<List<SuggestHitUi>>>(Resource.Idle)
    val suggest: StateFlow<Resource<List<SuggestHitUi>>> = _suggest.asStateFlow()

    private val _sources = MutableStateFlow<Resource<List<SourceEntryUi>>>(Resource.Idle)
    val sources: StateFlow<Resource<List<SourceEntryUi>>> = _sources.asStateFlow()

    private val _domains = MutableStateFlow<Resource<List<DomainFacetUi>>>(Resource.Idle)
    val domains: StateFlow<Resource<List<DomainFacetUi>>> = _domains.asStateFlow()

    private val _stats = MutableStateFlow<Resource<JsonElement>>(Resource.Idle)
    val stats: StateFlow<Resource<JsonElement>> = _stats.asStateFlow()

    private var suggestCachedAt: Long? = null
    private var sourcesCachedAt: Long? = null
    private var domainsCachedAt: Long? = null
    private var statsCachedAt: Long? = null

    private fun fresh(at: Long?): Boolean =
        at != null && (System.currentTimeMillis() - at) < FRESHNESS_MS

    fun loadSuggest(focus: String?, force: Boolean = false) {
        if (!force && fresh(suggestCachedAt) && _suggest.value is Resource.Ready) return
        viewModelScope.launch {
            _suggest.value = Resource.Loading
            val collection = container.settingsRepository.settings.first().collection
            container.axonRepository.suggest(focus = focus, collection = collection).fold(
                onSuccess = {
                    _suggest.value = Resource.Ready(it)
                    suggestCachedAt = System.currentTimeMillis()
                },
                onFailure = {
                    Log.w(TAG, "suggest failed", it)
                    _suggest.value = Resource.Error(it.message ?: "Error")
                    suggestCachedAt = null
                },
            )
        }
    }

    fun loadSources(force: Boolean = false) {
        if (!force && fresh(sourcesCachedAt) && _sources.value is Resource.Ready) return
        viewModelScope.launch {
            _sources.value = Resource.Loading
            val collection = container.settingsRepository.settings.first().collection
            container.axonRepository.sources(collection = collection).fold(
                onSuccess = {
                    _sources.value = Resource.Ready(it)
                    sourcesCachedAt = System.currentTimeMillis()
                },
                onFailure = {
                    Log.w(TAG, "sources failed", it)
                    _sources.value = Resource.Error(it.message ?: "Error")
                    sourcesCachedAt = null
                },
            )
        }
    }

    fun loadDomains(limit: Int = 200, force: Boolean = false) {
        if (!force && fresh(domainsCachedAt) && _domains.value is Resource.Ready) return
        viewModelScope.launch {
            _domains.value = Resource.Loading
            container.axonRepository.domains(limit = limit).fold(
                onSuccess = {
                    _domains.value = Resource.Ready(it)
                    domainsCachedAt = System.currentTimeMillis()
                },
                onFailure = {
                    Log.w(TAG, "domains failed", it)
                    _domains.value = Resource.Error(it.message ?: "Error")
                    domainsCachedAt = null
                },
            )
        }
    }

    fun loadStats(force: Boolean = false) {
        if (!force && fresh(statsCachedAt) && _stats.value is Resource.Ready) return
        viewModelScope.launch {
            _stats.value = Resource.Loading
            container.axonRepository.statsPayload().fold(
                onSuccess = {
                    _stats.value = Resource.Ready(it)
                    statsCachedAt = System.currentTimeMillis()
                },
                onFailure = {
                    Log.w(TAG, "stats failed", it)
                    _stats.value = Resource.Error(it.message ?: "Error")
                    statsCachedAt = null
                },
            )
        }
    }
}
