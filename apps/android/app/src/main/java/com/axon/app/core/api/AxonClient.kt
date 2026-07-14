package com.axon.app.core.api

import android.util.Log
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import com.axon.app.core.auth.AuthConfig
import com.axon.app.core.auth.hasUsableAuth
import com.axon.app.core.api.models.AcceptedJob
import com.axon.app.core.api.models.CancelResponse
import com.axon.app.core.api.models.DoctorResponse
import com.axon.app.core.api.models.DomainIndexedResponse
import com.axon.app.core.api.models.DomainsResponse
import com.axon.app.core.api.models.EmbedRequest
import com.axon.app.core.api.models.ExtractRequest
import com.axon.app.core.api.models.IngestRequest
import com.axon.app.core.api.models.JobSummaryPage
import com.axon.app.core.api.models.SearchWebRequest
import com.axon.app.core.api.models.SearchWebResponse
import com.axon.app.core.api.models.ServiceJob
import com.axon.app.core.api.models.SourceRequest
import com.axon.app.core.api.models.SourceRequestLimits
import com.axon.app.core.api.models.SourceResult
import com.axon.app.core.api.models.StatusSummary
import com.axon.app.core.api.models.SuggestRequest
import com.axon.app.core.api.models.SuggestResponse
import com.axon.app.core.api.models.SummarizeRequest
import com.axon.app.core.api.models.SummarizeResponse
import com.axon.app.core.api.models.UnifiedJobCancelResult
import com.axon.app.core.api.models.UnifiedJobSummary
import com.axon.app.core.api.models.WatchDef
import com.axon.app.core.api.models.WatchListResponse
import com.axon.app.core.api.models.toServiceJob
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.util.concurrent.atomic.AtomicReference
import java.net.URLEncoder

// ─────────────────────────────────────────────────────────────────────────────

// internal (not private): shared with the AxonClientStreaming.kt extension
// functions in this package, which need the same wire encoding + log tag.
internal val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()

internal val json = Json {
    ignoreUnknownKeys = true
    coerceInputValues = true
}

internal const val TAG = "AxonClient"

private fun queryEncode(value: String): String = URLEncoder.encode(value, "UTF-8")

class AxonClient(
    baseUrl: String,
    token: String,
) {
    // Thread-safe config: both baseUrl and auth mode updated atomically together.
    // internal (not private): read by the AxonClientMemory/Panel/Mobile/Streaming.kt
    // extension functions in this package via the internal helpers below.
    internal val config = AtomicReference<Pair<String, AuthConfig>>(baseUrl.trimEnd('/') to AuthConfig.Bearer(token))

    private val clients = AxonHttpClients()
    private val http = clients.normal
    private val httpLong = clients.longRead
    internal val httpStream = clients.stream
    internal val generatedApi = GeneratedAxonApi(
        snapshotProvider = { config.get().let { (baseUrl, auth) -> ClientAuthSnapshot(baseUrl, auth) } },
        clients = clients,
    )

    fun updateConfig(newBaseUrl: String, newToken: String) {
        updateConfig(newBaseUrl, AuthConfig.Bearer(newToken))
    }

    fun updateConfig(newBaseUrl: String, authConfig: AuthConfig) {
        config.set(newBaseUrl.trimEnd('/') to authConfig)
    }

    fun hasToken(): Boolean = hasUsableAuth()

    fun hasUsableAuth(): Boolean = config.get().second.hasUsableAuth()

    /**
     * Checks server reachability. Returns [Result.success] on HTTP 2xx,
     * [Result.failure] with the underlying cause otherwise so callers can show
     * the specific reason (401 Unauthorized, DNS failure, TLS error, etc.)
     * instead of the generic "Server unreachable".
     */
    suspend fun healthz(): Result<Unit> = withContext(Dispatchers.IO) {
        runCatching {
            val (baseUrl, _) = config.get()
            val req = Request.Builder()
                .url("$baseUrl/healthz")
                .get()
                .build()
            http.newCall(req).execute().use { resp ->
                if (!resp.isSuccessful) {
                    error(httpErrorMessage(resp.code, resp.body?.string(), resp.message))
                }
            }
        }.onFailure { if (it is CancellationException) throw it }
    }

    suspend fun ask(request: AskRequest): Result<AskResponse> = withContext(Dispatchers.IO) {
        post(openApiRoute("POST", "/v1/ask"), request)
    }

    suspend fun chat(request: ChatRequest): Result<ChatResponse> = withContext(Dispatchers.IO) {
        post(openApiRoute("POST", "/v1/chat"), request)
    }

    suspend fun query(request: QueryRequest): Result<QueryResponse> = withContext(Dispatchers.IO) {
        post(openApiRoute("POST", "/v1/query"), request)
    }

    suspend fun retrieve(request: RetrieveRequest): Result<RetrieveResponse> = withContext(Dispatchers.IO) {
        // Retrieve can return large assembled documents; use the longer-timeout client.
        postWith(httpLong, openApiRoute("POST", "/v1/retrieve"), request)
    }

    suspend fun sources(request: SourcesRequest = SourcesRequest()): Result<SourcesResponse> =
        withContext(Dispatchers.IO) {
            val params = buildList {
                add("limit=${request.limit}")
                add("offset=${request.offset}")
                request.domain?.takeIf { it.isNotBlank() }?.let { add("domain=${queryEncode(it)}") }
                request.cursor?.takeIf { it.isNotBlank() }?.let { add("cursor=${queryEncode(it)}") }
            }.joinToString("&")
            get(openApiRoute("GET", "/v1/sources", "/v1/sources?$params"))
        }

    suspend fun stats(): Result<StatsResponse> = withContext(Dispatchers.IO) {
        get(openApiRoute("GET", "/v1/stats"))
    }

    /**
     * Scrapes one URL through the unified `POST /v1/sources` pipeline (the
     * legacy `/v1/scrape` route hard-404s — see `rest_tests.rs`). Content is
     * returned only when the server resolves the fetch inline; otherwise
     * [ScrapeResponse.markdown] is empty and [ScrapeResponse.output] mirrors
     * [SourceResult.canonicalUri] for display.
     */
    suspend fun scrape(request: ScrapeRequest): Result<ScrapeResponse> = withContext(Dispatchers.IO) {
        submitSource(
            source = request.url,
            embed = request.embed,
            collection = request.collection,
        ).map { r ->
            val text = r.inline?.content?.takeIf { it.kind == "inline_text" }?.text
            ScrapeResponse(
                url = r.canonicalUri.ifBlank { request.url },
                markdown = text ?: "",
                output = text ?: r.canonicalUri,
            )
        }
    }

    suspend fun map(request: MapRequest): Result<MapResponse> = withContext(Dispatchers.IO) {
        post(openApiRoute("POST", "/v1/map"), request)
    }

    suspend fun research(request: ResearchRequest): Result<ResearchResponse> = withContext(Dispatchers.IO) {
        postWith(httpLong, openApiRoute("POST", "/v1/research"), request)
    }

    /**
     * Submits a crawl through the unified `POST /v1/sources` pipeline (the
     * legacy `/v1/crawl` route hard-404s). The source pipeline accepts a
     * single source string per request, so only [CrawlRequest.urls]' first
     * entry is submitted — a pre-existing limitation of the one-source-per-
     * request contract, not something this route migration introduces.
     */
    suspend fun crawlSubmit(request: CrawlRequest): Result<CrawlJobResponse> = withContext(Dispatchers.IO) {
        val startUrl = request.urls.firstOrNull().orEmpty()
        submitSource(
            source = startUrl,
            collection = request.collection,
            limits = SourceRequestLimits(
                maxPages = request.maxPages?.toLong(),
                maxDepth = request.maxDepth,
            ),
        ).map { r ->
            CrawlJobResponse(
                jobId = r.job?.id?.ifBlank { null } ?: r.jobId,
                url = r.canonicalUri.ifBlank { startUrl },
            )
        }
    }

    suspend fun crawlStatus(jobId: String): Result<CrawlStatusResponse> = withContext(Dispatchers.IO) {
        get<UnifiedJobSummary>(
            openApiRoute("GET", "/v1/jobs/{id}", "/v1/jobs/${encodePathSegment(jobId)}"),
        ).map { job ->
            CrawlStatusResponse(
                id = job.jobId,
                status = job.status,
                error = job.lastError?.toString(),
            )
        }
    }

    // ── Phase 2 endpoints ──────────────────────────────────────────────────────

    enum class JobKind(val path: String) {
        Crawl("crawl"), Embed("embed"), Extract("extract"), Ingest("ingest")
    }

    /** /v1/summarize — Gemini-backed, can take minutes. Use httpLong. */
    suspend fun summarize(req: SummarizeRequest): Result<SummarizeResponse> = withContext(Dispatchers.IO) {
        postWith(httpLong, openApiRoute("POST", "/v1/summarize"), req)
    }

    /** /v1/search — Tavily web search; auto-enqueues crawl jobs server-side. */
    suspend fun searchWeb(req: SearchWebRequest): Result<SearchWebResponse> = withContext(Dispatchers.IO) {
        post(openApiRoute("POST", "/v1/search"), req)
    }

    /**
     * Submits an ingest target through the unified `POST /v1/sources`
     * pipeline (the legacy `/v1/ingest` route hard-404s). `req.sourceType`
     * was a routing hint for the old family-specific endpoint; the source
     * pipeline classifies the target itself, so it is not sent.
     */
    suspend fun ingestStart(req: IngestRequest): Result<AcceptedJob> = withContext(Dispatchers.IO) {
        submitSource(source = req.target.orEmpty()).map { it.toAcceptedJob() }
    }

    /** POST /v1/extract — submits an async structured extraction job. */
    suspend fun extractStart(req: ExtractRequest): Result<AcceptedJob> = withContext(Dispatchers.IO) {
        post(openApiRoute("POST", "/v1/extract"), req)
    }

    /**
     * Submits a local-path or text embed through the unified
     * `POST /v1/sources` pipeline (the legacy `/v1/embed` route hard-404s).
     */
    suspend fun embedStart(req: EmbedRequest): Result<AcceptedJob> = withContext(Dispatchers.IO) {
        submitSource(source = req.input, collection = req.collection).map { it.toAcceptedJob() }
    }

    /** Shared POST /v1/sources call — see [SourceRequest]/[SourceResult]. */
    private suspend fun submitSource(
        source: String,
        embed: Boolean? = null,
        collection: String? = null,
        limits: SourceRequestLimits? = null,
    ): Result<SourceResult> = post(
        openApiRoute("POST", "/v1/sources"),
        SourceRequest(source = source, embed = embed, collection = collection, limits = limits),
    )

    /** Maps [SourceResult] into the legacy [AcceptedJob] shape (`job_id`/`status`/`status_url`). */
    private fun SourceResult.toAcceptedJob(): AcceptedJob = AcceptedJob(
        jobId = job?.id?.ifBlank { null } ?: jobId,
        status = status.ifBlank { "pending" },
        statusUrl = job?.statusUrl,
    )

    /** GET /v1/jobs/{id} — unified job detail. Long-poll-friendly via httpLong. */
    suspend fun getJob(kind: JobKind, id: String): Result<ServiceJob> = withContext(Dispatchers.IO) {
        getWith<UnifiedJobSummary>(
            httpLong,
            openApiRoute("GET", "/v1/jobs/{id}", "/v1/jobs/${encodePathSegment(id)}"),
        ).map { it.toServiceJob() }
    }

    /** GET /v1/jobs?kind=... — list unified jobs filtered to one kind. */
    suspend fun listJobs(kind: JobKind, limit: Int = 25, offset: Int = 0): Result<List<ServiceJob>> = withContext(Dispatchers.IO) {
        get<JobSummaryPage>(
            openApiRoute("GET", "/v1/jobs", "/v1/jobs?kind=${queryEncode(kind.path)}&limit=$limit"),
        ).map { page -> page.items.map { it.toServiceJob() } }
    }

    /** POST /v1/jobs/{id}/cancel. */
    suspend fun cancelJob(kind: JobKind, id: String): Result<CancelResponse> = withContext(Dispatchers.IO) {
        val body = "{}".toRequestBody(JSON_MEDIA_TYPE)
        val builder = runCatching {
            authRequest(
                Request.Builder()
                    .url("${baseUrl()}${openApiRoute("POST", "/v1/jobs/{id}/cancel", "/v1/jobs/${encodePathSegment(id)}/cancel")}")
                    .post(body),
            )
        }.getOrElse { return@withContext Result.failure(it) }
        execute<UnifiedJobCancelResult>(http, builder).map { result ->
            val normalized = result.status.lowercase()
            CancelResponse(
                canceled = normalized in setOf("cancelled", "canceled", "cancelling", "canceling"),
            )
        }
    }

    suspend fun status(): Result<StatusSummary> = withContext(Dispatchers.IO) { get(openApiRoute("GET", "/v1/status")) }

    suspend fun doctor(): Result<DoctorResponse> = withContext(Dispatchers.IO) { get(openApiRoute("GET", "/v1/doctor")) }

    suspend fun suggest(focus: String? = null, collection: String? = null): Result<SuggestResponse> =
        withContext(Dispatchers.IO) { post(openApiRoute("POST", "/v1/suggest"), SuggestRequest(focus = focus, collection = collection)) }

    suspend fun domains(limit: Int = 100, offset: Int = 0): Result<DomainsResponse> =
        withContext(Dispatchers.IO) {
            get(openApiRoute("GET", "/v1/domains", "/v1/domains?limit=$limit&offset=$offset"))
        }

    suspend fun domainIndexed(domain: String): Result<DomainIndexedResponse> =
        withContext(Dispatchers.IO) {
            get(openApiRoute("GET", "/v1/domains", "/v1/domains?domain=${queryEncode(domain)}"))
        }

    suspend fun listWatches(limit: Int = 25): Result<List<WatchDef>> = withContext(Dispatchers.IO) {
        get<WatchListResponse>(openApiRoute("GET", "/v1/watch", "/v1/watch?limit=$limit")).map { it.watches }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────
    // Several of these are `internal` rather than `private`: the AxonClientMemory/
    // Panel/Mobile/Streaming.kt extension functions in this package call them
    // directly and need at least module visibility to do so. Public method
    // signatures elsewhere in this class are unaffected.

    internal fun encodePathSegment(s: String): String =
        java.net.URLEncoder.encode(s, "UTF-8").replace("+", "%20")

    internal suspend fun authRequest(builder: Request.Builder, panelRoute: Boolean = false): Request.Builder {
        val snapshot = config.get().let { (baseUrl, auth) -> ClientAuthSnapshot(baseUrl, auth) }
        return builder.withAxonAuth(snapshot, panelRoute)
    }

    internal suspend inline fun <reified B, reified R> put(path: String, body: B): Result<R> {
        val bodyBytes = json.encodeToString(body).toRequestBody(JSON_MEDIA_TYPE)
        val builder = runCatching {
            authRequest(
                Request.Builder().url("${baseUrl()}$path").put(bodyBytes),
                panelRoute = path.startsWith("/api/panel/"),
            )
        }.getOrElse { return Result.failure(it) }
        return execute(http, builder)
    }

    internal suspend inline fun <reified R> get(path: String): Result<R> {
        val builder = runCatching {
            authRequest(
                Request.Builder().url("${baseUrl()}$path").get(),
                panelRoute = path.startsWith("/api/panel/"),
            )
        }.getOrElse { return Result.failure(it) }
        return execute(http, builder)
    }

    internal suspend inline fun <reified R> delete(path: String): Result<R> {
        val builder = runCatching {
            authRequest(
                Request.Builder().url("${baseUrl()}$path").delete(),
                panelRoute = path.startsWith("/api/panel/"),
            )
        }.getOrElse { return Result.failure(it) }
        return execute(http, builder)
    }

    internal suspend fun getText(path: String): Result<String> {
        val builder = runCatching {
            authRequest(
                Request.Builder().url("${baseUrl()}$path").get(),
                panelRoute = path.startsWith("/api/panel/"),
            )
        }.getOrElse { return Result.failure(it) }
        return executeText(http, builder)
    }

    private suspend inline fun <reified R> getWith(client: OkHttpClient, path: String): Result<R> {
        val builder = runCatching {
            authRequest(
                Request.Builder().url("${baseUrl()}$path").get(),
                panelRoute = path.startsWith("/api/panel/"),
            )
        }.getOrElse { return Result.failure(it) }
        return execute(client, builder)
    }

    internal fun baseUrl(): String = config.get().first

    internal fun openApiRoute(method: String, template: String, resolved: String = template): String {
        require(method == "GET" || method == "POST" || method == "PUT" || method == "DELETE")
        require(template.startsWith("/v1/"))
        require(resolved.startsWith("/v1/"))
        return resolved
    }

    internal suspend inline fun <reified B, reified R> post(path: String, body: B): Result<R> =
        postWith(http, path, body)

    private suspend inline fun <reified B, reified R> postWith(client: OkHttpClient, path: String, body: B): Result<R> {
        val bodyBytes = json.encodeToString(body).toRequestBody(JSON_MEDIA_TYPE)
        val request = Request.Builder().url("${baseUrl()}$path").post(bodyBytes)
        val builder = runCatching {
            when {
                path == "/api/panel/login" -> request
                else -> authRequest(request, panelRoute = path.startsWith("/api/panel/"))
            }
        }.getOrElse { return Result.failure(it) }
        return execute(client, builder)
    }

    // internal (not private): `get`/`post`/`put`/`delete` above are `internal inline`
    // (called from sibling extension-function files), and an internal inline
    // function cannot reference a strictly-private member of its own class —
    // the compiler forbids leaking private bytecode through an inlined call site.
    internal inline fun <reified R> execute(client: OkHttpClient, builder: Request.Builder): Result<R> {
        val built = builder.build()
        return runCatching {
            client.newCall(built).execute().use { resp ->
                if (!resp.isSuccessful) {
                    error(httpErrorMessage(resp.code, resp.body?.string(), resp.message))
                }
                // Read body exactly once — use() closes the response, so the stream is single-pass.
                json.decodeFromString<R>(resp.body?.string() ?: error("Empty response body"))
            }
        }.onFailure { t ->
            if (t is CancellationException) throw t
            // One-line logcat breadcrumb for any non-cancellation failure (HTTP error,
            // decode mismatch, transport error). Body is truncated upstream; method+path
            // is enough to grep when triaging field reports.
            Log.w(TAG, "${built.method} ${built.url.encodedPath} failed: ${t.message}")
        }
    }

    internal fun executeText(client: OkHttpClient, builder: Request.Builder): Result<String> {
        val built = builder.build()
        return runCatching {
            client.newCall(built).execute().use { resp ->
                if (!resp.isSuccessful) {
                    error(httpErrorMessage(resp.code, resp.body?.string(), resp.message))
                }
                resp.body?.string() ?: error("Empty response body")
            }
        }.onFailure { t ->
            if (t is CancellationException) throw t
            Log.w(TAG, "${built.method} ${built.url.encodedPath} failed: ${t.message}")
        }
    }

}
