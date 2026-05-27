package com.axon.app.ui.tools

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.remote.ResearchHit
import com.axon.app.data.repository.CrawlStatusUi
import com.axon.app.data.repository.MapResultUi
import com.axon.app.data.repository.ResearchResultUi
import com.axon.app.data.repository.ScrapeResultUi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

// ── Scrape ────────────────────────────────────────────────────────────────────

sealed interface ScrapeUiState {
    object Idle : ScrapeUiState
    object Loading : ScrapeUiState
    data class Success(val result: ScrapeResultUi) : ScrapeUiState
    data class Error(val message: String) : ScrapeUiState
}

// ── Map ───────────────────────────────────────────────────────────────────────

sealed interface MapUiState {
    object Idle : MapUiState
    object Loading : MapUiState
    data class Success(val result: MapResultUi) : MapUiState
    data class Error(val message: String) : MapUiState
}

// ── Research ──────────────────────────────────────────────────────────────────

sealed interface ResearchUiState {
    object Idle : ResearchUiState
    object Loading : ResearchUiState
    data class Success(val result: ResearchResultUi) : ResearchUiState
    data class Error(val message: String) : ResearchUiState
}

// ── Crawl ─────────────────────────────────────────────────────────────────────

sealed interface CrawlUiState {
    object Idle : CrawlUiState
    object Loading : CrawlUiState
    data class Submitted(val jobId: String) : CrawlUiState
    /**
     * [serverError] is non-null when the Axon server reported a failure reason for the crawl job.
     * Surfacing it lets users understand why a crawl failed rather than just seeing "failed".
     */
    data class StatusPolled(
        val jobId: String,
        val status: String,
        val pagesCrawled: Int?,
        val serverError: String?,
    ) : CrawlUiState
    data class Error(val message: String) : CrawlUiState
}

// ── ViewModel ─────────────────────────────────────────────────────────────────

class ToolsViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container
    private val repo = container.axonRepository

    // Scrape state
    private val _scrapeState = MutableStateFlow<ScrapeUiState>(ScrapeUiState.Idle)
    val scrapeState: StateFlow<ScrapeUiState> = _scrapeState.asStateFlow()

    // Map state
    private val _mapState = MutableStateFlow<MapUiState>(MapUiState.Idle)
    val mapState: StateFlow<MapUiState> = _mapState.asStateFlow()

    // Research state
    private val _researchState = MutableStateFlow<ResearchUiState>(ResearchUiState.Idle)
    val researchState: StateFlow<ResearchUiState> = _researchState.asStateFlow()

    // Crawl state
    private val _crawlState = MutableStateFlow<CrawlUiState>(CrawlUiState.Idle)
    val crawlState: StateFlow<CrawlUiState> = _crawlState.asStateFlow()

    fun scrape(url: String) {
        if (url.isBlank()) return
        viewModelScope.launch {
            _scrapeState.value = ScrapeUiState.Loading
            val collection = container.settingsRepository.settings.first().collection
            repo.scrape(url, collection = collection).fold(
                onSuccess = { _scrapeState.value = ScrapeUiState.Success(it) },
                onFailure = { _scrapeState.value = ScrapeUiState.Error(it.message ?: "Scrape failed") },
            )
        }
    }

    fun map(url: String) {
        if (url.isBlank()) return
        viewModelScope.launch {
            _mapState.value = MapUiState.Loading
            repo.map(url).fold(
                onSuccess = { _mapState.value = MapUiState.Success(it) },
                onFailure = { _mapState.value = MapUiState.Error(it.message ?: "Map failed") },
            )
        }
    }

    fun research(query: String) {
        if (query.isBlank()) return
        viewModelScope.launch {
            _researchState.value = ResearchUiState.Loading
            repo.research(query).fold(
                onSuccess = { _researchState.value = ResearchUiState.Success(it) },
                onFailure = { _researchState.value = ResearchUiState.Error(it.message ?: "Research failed") },
            )
        }
    }

    fun crawl(url: String, maxPages: Int? = null) {
        if (url.isBlank()) return
        viewModelScope.launch {
            _crawlState.value = CrawlUiState.Loading
            val collection = container.settingsRepository.settings.first().collection
            repo.crawlSubmit(url, maxPages, collection = collection).fold(
                onSuccess = { jobId -> _crawlState.value = CrawlUiState.Submitted(jobId) },
                onFailure = { _crawlState.value = CrawlUiState.Error(it.message ?: "Crawl submit failed") },
            )
        }
    }

    fun pollCrawlStatus(jobId: String) {
        viewModelScope.launch {
            repo.crawlStatus(jobId).fold(
                onSuccess = { s ->
                    _crawlState.value = CrawlUiState.StatusPolled(
                        jobId = s.jobId,
                        status = s.status,
                        pagesCrawled = s.pagesCrawled,
                        serverError = s.serverError,
                    )
                },
                onFailure = { _crawlState.value = CrawlUiState.Error(it.message ?: "Status check failed") },
            )
        }
    }
}
