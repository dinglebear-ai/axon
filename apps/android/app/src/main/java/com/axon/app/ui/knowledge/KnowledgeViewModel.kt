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
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonElement
import kotlin.reflect.KMutableProperty0

private const val TAG = "KnowledgeViewModel"
private const val FRESHNESS_MS = 30_000L

/**
 * Knowledge page ViewModel.
 *
 * Four independent sections — each backed by a [Resource]<T> StateFlow (R8):
 * - **Suggest** (`/v1/suggest`) — `List<SuggestHitUi>`
 * - **Sources** (`/v1/sources`) — `List<SourceEntryUi>`
 * - **Domains** (`/v1/domains`) — `List<DomainFacetUi>`
 * - **Stats**   (`/v1/stats`)   — `JsonElement` parsed into human-readable rows
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

    /**
     * Shared loader for the four sections. Pulls the persistent state, the
     * cache timestamp, and the fetch function out as parameters so the four
     * `loadX()` entry points collapse to two lines each.
     *
     * - `state`  — the section's MutableStateFlow (mutated through this fn only)
     * - `cachedAt` — the section's cache-timestamp field reference
     * - `force`  — bypasses the freshness short-circuit
     * - `label`  — log tag suffix so failure traces identify the section
     * - `fetch`  — suspending fetch (typically `container.axonRepository.X(...)`)
     */
    private fun <T> loadSection(
        state: MutableStateFlow<Resource<T>>,
        cachedAt: KMutableProperty0<Long?>,
        force: Boolean,
        label: String,
        fetch: suspend () -> Result<T>,
    ) {
        if (!force && fresh(cachedAt.get()) && state.value is Resource.Ready) return
        if (!force && state.value is Resource.Loading) return
        viewModelScope.launch {
            state.value = Resource.Loading
            fetch().fold(
                onSuccess = {
                    state.value = Resource.Ready(it)
                    cachedAt.set(System.currentTimeMillis())
                },
                onFailure = {
                    Log.w(TAG, "$label failed", it)
                    state.value = Resource.Error(it.message ?: "Error")
                    cachedAt.set(null)
                },
            )
        }
    }

    fun loadSuggest(focus: String?, force: Boolean = false) {
        loadSection(_suggest, ::suggestCachedAt, force, "suggest") {
            val collection = container.settingsRepository.settings.first().collection
            container.axonRepository.suggest(focus = focus, collection = collection)
        }
    }

    fun loadSources(force: Boolean = false) {
        loadSection(_sources, ::sourcesCachedAt, force, "sources") {
            container.axonRepository.sources()
        }
    }

    fun loadDomains(limit: Int = 200, force: Boolean = false) {
        loadSection(_domains, ::domainsCachedAt, force, "domains") {
            container.axonRepository.domains(limit = limit)
        }
    }

    fun loadStats(force: Boolean = false) {
        loadSection(_stats, ::statsCachedAt, force, "stats") {
            container.axonRepository.statsPayload()
        }
    }
}
