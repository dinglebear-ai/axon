package com.axon.app.data.repository

import android.content.Context
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import com.axon.app.core.api.AskRequest
import com.axon.app.core.api.CrawlRequest
import com.axon.app.core.api.MapRequest
import com.axon.app.core.api.QueryRequest
import com.axon.app.core.api.ResearchRequest
import com.axon.app.core.api.ScrapeRequest
import com.axon.app.core.api.models.IngestRequest
import com.axon.app.core.api.models.SearchWebRequest
import com.axon.app.core.api.models.SummarizeRequest
import com.axon.app.data.repository.options.AskFormKeys
import com.axon.app.data.repository.options.CrawlFormKeys
import com.axon.app.data.repository.options.IngestFormKeys
import com.axon.app.data.repository.options.MapFormKeys
import com.axon.app.data.repository.options.QueryFormKeys
import com.axon.app.data.repository.options.ResearchFormKeys
import com.axon.app.data.repository.options.ScrapeFormKeys
import com.axon.app.data.repository.options.SearchWebFormKeys
import com.axon.app.data.repository.options.SummarizeFormKeys
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first

/**
 * Generic DataStore<Preferences> helpers plus the [ModeOptionsApplicator] impl.
 *
 * Per the R9 contract from /lavra-eng-review, this class is intentionally tiny:
 *   - generic [read]/[write] helpers exposed to forms
 *   - one `apply()` per wire DTO that merges persisted overrides
 *
 * Each form file owns its own `intPreferencesKey(...)` set + defaults. The applicator
 * reaches into those keys via the `*FormKeys` objects exported from each form file —
 * the form is still the single source of truth for which keys exist.
 */
class ModeOptionsRepository(
    private val context: Context,
    private val encryptedHeadersStore: EncryptedHeadersStore = EncryptedHeadersStore(context),
) : ModeOptionsApplicator {

    private val _resetVersion = MutableStateFlow(0)
    /** Incremented each time [resetKeys] clears DataStore; forms re-read their stored values. */
    val resetVersion: StateFlow<Int> = _resetVersion.asStateFlow()

    /** Read a single Preferences value once (no flow). Returns null when unset. */
    suspend fun <T> read(key: Preferences.Key<T>): T? =
        context.modeOptionsDataStore.data.first()[key]

    // ── Encrypted-header convenience API ─────────────────────────────────────
    //
    // Headers can carry bearer tokens / cookies / API keys, so they must NOT
    // live in the plaintext mode-options DataStore. These wrappers delegate to
    // [EncryptedHeadersStore]; forms call them directly instead of using the
    // generic Preferences read/write helpers above for header lists.

    fun readEncryptedHeaders(key: String): List<String> =
        encryptedHeadersStore.read(key).orEmpty()

    fun writeEncryptedHeaders(key: String, headers: List<String>) {
        encryptedHeadersStore.write(key, headers)
    }

    /** Persist a single value. Pass `null` to remove the key (reset to default). */
    suspend fun <T> write(key: Preferences.Key<T>, value: T?) {
        context.modeOptionsDataStore.edit { prefs ->
            if (value == null) prefs.remove(key) else prefs[key] = value
        }
    }

    /** Bulk-remove a key set — used by per-form "Reset to defaults" buttons. */
    suspend fun resetKeys(keys: List<Preferences.Key<*>>) {
        context.modeOptionsDataStore.edit { prefs ->
            keys.forEach { prefs.remove(it) }
        }
        _resetVersion.value++
    }

    // ── Applicator implementations ───────────────────────────────────────────
    //
    // Each apply() reads only keys for its own mode. Unset keys leave the
    // existing request field untouched so call-site arguments still win when
    // the user has not configured an override.

    override suspend fun apply(req: AskRequest): AskRequest {
        val prefs = context.modeOptionsDataStore.data.first()
        return req.copy(
            chunkLimit       = req.chunkLimit       ?: prefs[AskFormKeys.CHUNK_LIMIT],
            fullDocs         = req.fullDocs         ?: prefs[AskFormKeys.FULL_DOCS],
            maxContextChars  = req.maxContextChars  ?: prefs[AskFormKeys.MAX_CONTEXT_CHARS],
            hybridCandidates = req.hybridCandidates ?: prefs[AskFormKeys.HYBRID_CANDIDATES],
            diagnostics      = req.diagnostics      ?: prefs[AskFormKeys.DIAGNOSTICS],
            explain          = req.explain          ?: prefs[AskFormKeys.EXPLAIN],
            collection       = req.collection       ?: prefs[AskFormKeys.COLLECTION],
        )
    }

    override suspend fun apply(req: QueryRequest): QueryRequest {
        val prefs = context.modeOptionsDataStore.data.first()
        val limitOverride = prefs[QueryFormKeys.LIMIT]
        // Caller wins when they passed a non-default limit; only apply the stored
        // form override when the caller left limit at its default value (10).
        return req.copy(
            limit      = req.limit.takeIf { it != 10 } ?: limitOverride ?: req.limit,
            collection = req.collection ?: prefs[QueryFormKeys.COLLECTION],
        )
    }

    override suspend fun apply(req: SummarizeRequest): SummarizeRequest {
        val prefs = context.modeOptionsDataStore.data.first()
        return req.copy(
            renderMode      = req.renderMode      ?: prefs[SummarizeFormKeys.RENDER_MODE],
            rootSelector    = req.rootSelector    ?: prefs[SummarizeFormKeys.ROOT_SELECTOR]?.takeIf { it.isNotBlank() },
            excludeSelector = req.excludeSelector ?: prefs[SummarizeFormKeys.EXCLUDE_SELECTOR]?.takeIf { it.isNotBlank() },
        )
    }

    override suspend fun apply(req: ResearchRequest): ResearchRequest {
        val prefs = context.modeOptionsDataStore.data.first()
        return req.copy(
            limit = req.limit ?: prefs[ResearchFormKeys.LIMIT],
        )
    }

    override suspend fun apply(req: ScrapeRequest): ScrapeRequest {
        val prefs = context.modeOptionsDataStore.data.first()
        return req.copy(
            renderMode = req.renderMode ?: prefs[ScrapeFormKeys.RENDER_MODE],
            format     = req.format     ?: prefs[ScrapeFormKeys.FORMAT],
            embed      = req.embed      ?: prefs[ScrapeFormKeys.EMBED],
            collection = req.collection ?: prefs[ScrapeFormKeys.COLLECTION],
        )
    }

    override suspend fun apply(req: CrawlRequest): CrawlRequest {
        val prefs = context.modeOptionsDataStore.data.first()
        // Headers come from EncryptedHeadersStore — never the plaintext DataStore.
        val headers = if (req.headers.isNotEmpty()) req.headers
                      else readEncryptedHeaders(EncryptedHeadersStore.KEY_CRAWL_HEADERS)
                          .filter { it.isNotBlank() }
        return req.copy(
            maxPages          = req.maxPages          ?: prefs[CrawlFormKeys.MAX_PAGES],
            maxDepth          = req.maxDepth          ?: prefs[CrawlFormKeys.MAX_DEPTH],
            renderMode        = req.renderMode        ?: prefs[CrawlFormKeys.RENDER_MODE],
            includeSubdomains = req.includeSubdomains ?: prefs[CrawlFormKeys.INCLUDE_SUBDOMAINS],
            collection        = req.collection        ?: prefs[CrawlFormKeys.COLLECTION],
            headers           = headers,
        )
    }

    override suspend fun apply(req: MapRequest): MapRequest {
        val prefs = context.modeOptionsDataStore.data.first()
        return req.copy(
            limit  = req.limit  ?: prefs[MapFormKeys.LIMIT],
            offset = req.offset ?: prefs[MapFormKeys.OFFSET],
        )
    }

    override suspend fun apply(req: SearchWebRequest): SearchWebRequest {
        val prefs = context.modeOptionsDataStore.data.first()
        return req.copy(
            limit     = req.limit     ?: prefs[SearchWebFormKeys.LIMIT],
            offset    = req.offset    ?: prefs[SearchWebFormKeys.OFFSET],
            timeRange = req.timeRange ?: prefs[SearchWebFormKeys.TIME_RANGE]?.takeIf { it.isNotBlank() },
        )
    }

    override suspend fun apply(req: IngestRequest): IngestRequest {
        val prefs = context.modeOptionsDataStore.data.first()
        return req.copy(
            includeSource = req.includeSource ?: prefs[IngestFormKeys.INCLUDE_SOURCE],
        )
    }
}
