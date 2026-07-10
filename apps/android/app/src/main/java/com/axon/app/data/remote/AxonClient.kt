package com.axon.app.data.remote

import android.util.Log
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.job
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import com.axon.app.data.auth.AuthConfig
import com.axon.app.data.auth.hasUsableAuth
import com.axon.app.data.remote.models.AcceptedJob
import com.axon.app.data.remote.models.CancelResponse
import com.axon.app.data.remote.models.DoctorResponse
import com.axon.app.data.remote.models.DomainIndexedResponse
import com.axon.app.data.remote.models.DomainsResponse
import com.axon.app.data.remote.models.EmbedRequest
import com.axon.app.data.remote.models.ExtractRequest
import com.axon.app.data.remote.models.IngestRequest
import com.axon.app.data.remote.models.JobDetailResponse
import com.axon.app.data.remote.models.JobListResponse
import com.axon.app.data.remote.models.MobileSessionDto
import com.axon.app.data.remote.models.PanelConfigResponse
import com.axon.app.data.remote.models.PanelCollectionsResponse
import com.axon.app.data.remote.models.PanelEnvResponse
import com.axon.app.data.remote.models.SavePanelConfigRequest
import com.axon.app.data.remote.models.SavePanelConfigResponse
import com.axon.app.data.remote.models.SavePanelEnvRequest
import com.axon.app.data.remote.models.SearchWebRequest
import com.axon.app.data.remote.models.SearchWebResponse
import com.axon.app.data.remote.models.ServiceJob
import com.axon.app.data.remote.models.SourceRequest
import com.axon.app.data.remote.models.SourceRequestLimits
import com.axon.app.data.remote.models.SourceResult
import com.axon.app.data.remote.models.StatusSummary
import com.axon.app.data.remote.models.SuggestRequest
import com.axon.app.data.remote.models.SuggestResponse
import com.axon.app.data.remote.models.SummarizeRequest
import com.axon.app.data.remote.models.SummarizeResponse
import com.axon.app.data.remote.models.WatchDef
import com.axon.app.data.remote.models.WatchListResponse
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.util.concurrent.atomic.AtomicReference
import java.net.URLEncoder

// ─────────────────────────────────────────────────────────────────────────────

private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()

private val json = Json {
    ignoreUnknownKeys = true
    coerceInputValues = true
}

private const val TAG = "AxonClient"

private fun queryEncode(value: String): String = URLEncoder.encode(value, "UTF-8")

class AxonClient(
    baseUrl: String,
    token: String,
) {
    // Thread-safe config: both baseUrl and auth mode updated atomically together.
    private val config = AtomicReference<Pair<String, AuthConfig>>(baseUrl.trimEnd('/') to AuthConfig.Bearer(token))

    private val clients = AxonHttpClients()
    private val http = clients.normal
    private val httpLong = clients.longRead
    private val httpStream = clients.stream
    private val generatedApi = GeneratedAxonApi(
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

    /**
     * Streams the ask response via SSE from POST /v1/ask/stream.
     * Emits [AskStreamEvent.Meta] for phase indicators, [AskStreamEvent.Delta] for each LLM token,
     * [AskStreamEvent.Done] when synthesis completes, and [AskStreamEvent.Error] on failure.
     *
     * Uses the dedicated [httpStream] client so the SSE idle timeout does not interfere with
     * regular request timeouts on [http].
     */
    fun askStream(request: AskRequest): Flow<AskStreamEvent> = flow {
        emitAll(streamCompletion(openApiRoute("POST", "/v1/ask/stream"), request))
    }.flowOn(Dispatchers.IO)

    fun chatStream(request: ChatRequest): Flow<AskStreamEvent> = flow {
        emitAll(streamCompletion(openApiRoute("POST", "/v1/chat/stream"), request))
    }.flowOn(Dispatchers.IO)

    private inline fun <reified T> streamCompletion(path: String, request: T): Flow<AskStreamEvent> = flow {
        val bodyBytes = json.encodeToString(request).toRequestBody(JSON_MEDIA_TYPE)
        // Capture atomically once — avoids a TOCTOU race if updateConfig() is called mid-stream.
        val requestBuilder = runCatching {
            authRequest(
                Request.Builder()
                    .url("${baseUrl()}$path")
                    .post(bodyBytes),
            )
        }.getOrElse {
            emit(AskStreamEvent.Error(it.message ?: "No Axon authentication configured"))
            return@flow
        }
        val req = requestBuilder.build()

        // Capture the Call before execute() so we can cancel it from
        // invokeOnCompletion. Without this, BufferedReader.readLine() below blocks
        // an IO thread until the SSE socket idles out (STREAM_READ_TIMEOUT_SECONDS
        // = 300s) when the parent coroutine is cancelled — leaking threads on
        // every navigate-away mid-stream and stalling subsequent ask() calls.
        val call = httpStream.newCall(req)
        val cancelHandle = currentCoroutineContext().job.invokeOnCompletion {
            runCatching { call.cancel() }
        }

        val resp = try {
            call.execute()
        } catch (t: Throwable) {
            cancelHandle.dispose()
            if (t is CancellationException) throw t
            Log.w(TAG, "askStream: connect failed", t)
            emit(AskStreamEvent.Error(t.message ?: "Stream connect failed"))
            return@flow
        }
        try {
            if (!resp.isSuccessful) {
                val rawBody = resp.body?.string()
                val humanError = httpErrorMessage(resp.code, rawBody, resp.message)
                Log.w(TAG, "askStream: $humanError")
                emit(AskStreamEvent.Error(humanError))
                return@flow
            }
            val reader = resp.body?.byteStream()?.bufferedReader()
            if (reader == null) {
                emit(AskStreamEvent.Error("Empty response body"))
                return@flow
            }
            try {
                var line: String?
                while (reader.readLine().also { line = it } != null) {
                    val l = line ?: break
                    if (!l.startsWith("data: ")) continue
                    val data = l.removePrefix("data: ").trim()
                    if (data.isEmpty()) continue
                    val event = parseStreamEvent(data) ?: continue
                    emit(event)
                    if (event is AskStreamEvent.Done || event is AskStreamEvent.Error) break
                }
            } catch (t: Throwable) {
                // Socket closed mid-stream (cancel(), timeout, network drop). Surface as
                // a clean Error so callers can distinguish from a normal Done.
                if (t is CancellationException) throw t
                Log.w(TAG, "askStream: read failed mid-stream", t)
                emit(AskStreamEvent.Error(t.message ?: "Stream interrupted"))
            } finally {
                runCatching { reader.close() }
            }
        } finally {
            runCatching { resp.close() }
            cancelHandle.dispose()
        }
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
        // The server wraps the job in {"job": {...}}; decode the envelope and unwrap.
        get<CrawlStatusWrapper>(
            openApiRoute("GET", "/v1/crawl/{id}", "/v1/crawl/${encodePathSegment(jobId)}"),
        ).map { it.job }
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

    /** GET /v1/{kind}/{id} — job detail. Long-poll-friendly via httpLong. */
    suspend fun getJob(kind: JobKind, id: String): Result<ServiceJob> = withContext(Dispatchers.IO) {
        getWith<JobDetailResponse>(
            httpLong,
            openApiRoute("GET", "/v1/{kind}/{id}", "/v1/${kind.path}/${encodePathSegment(id)}"),
        ).map { it.job }
    }

    /** GET /v1/{kind} — list jobs of one kind. Server wraps in {"jobs":[...],"limit":N,"offset":N}. */
    suspend fun listJobs(kind: JobKind, limit: Int = 25, offset: Int = 0): Result<List<ServiceJob>> = withContext(Dispatchers.IO) {
        get<JobListResponse>(
            openApiRoute("GET", "/v1/{kind}", "/v1/${kind.path}?limit=$limit&offset=$offset"),
        ).map { it.jobs }
    }

    /** POST /v1/{kind}/{id}/cancel. */
    suspend fun cancelJob(kind: JobKind, id: String): Result<CancelResponse> = withContext(Dispatchers.IO) {
        val body = "{}".toRequestBody(JSON_MEDIA_TYPE)
        val builder = runCatching {
            authRequest(
                Request.Builder()
                    .url("${baseUrl()}${openApiRoute("POST", "/v1/{kind}/{id}/cancel", "/v1/${kind.path}/${encodePathSegment(id)}/cancel")}")
                    .post(body),
            )
        }.getOrElse { return@withContext Result.failure(it) }
        execute(http, builder)
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

    suspend fun listMobileSessions(): Result<List<MobileSessionDto>> = withContext(Dispatchers.IO) {
        generatedApi.listMobileSessions()
    }

    suspend fun getMobileSession(id: String): Result<MobileSessionDto> = withContext(Dispatchers.IO) {
        generatedApi.getMobileSession(id)
    }

    suspend fun upsertMobileSession(session: MobileSessionDto): Result<MobileSessionDto> = withContext(Dispatchers.IO) {
        generatedApi.upsertMobileSession(session)
    }

    suspend fun deleteMobileSession(id: String): Result<Boolean> = withContext(Dispatchers.IO) {
        generatedApi.deleteMobileSession(id)
    }

    suspend fun artifactText(relativePath: String): Result<String> = withContext(Dispatchers.IO) {
        val encodedPath = URLEncoder.encode(relativePath, "UTF-8").replace("+", "%20")
        getText(openApiRoute("GET", "/v1/artifacts", "/v1/artifacts?path=$encodedPath"))
    }

    suspend fun panelConfig(): Result<PanelConfigResponse> = withContext(Dispatchers.IO) {
        get("/api/panel/config")
    }

    suspend fun panelEnv(): Result<PanelEnvResponse> = withContext(Dispatchers.IO) {
        get("/api/panel/env")
    }

    suspend fun savePanelConfig(rawToml: String): Result<SavePanelConfigResponse> = withContext(Dispatchers.IO) {
        put("/api/panel/config", SavePanelConfigRequest(rawToml))
    }

    suspend fun savePanelEnv(rawEnv: String): Result<SavePanelConfigResponse> = withContext(Dispatchers.IO) {
        put("/api/panel/env", SavePanelEnvRequest(rawEnv))
    }

    suspend fun panelCollections(): Result<PanelCollectionsResponse> = withContext(Dispatchers.IO) {
        get("/api/panel/collections")
    }

    suspend fun collections(): Result<PanelCollectionsResponse> = withContext(Dispatchers.IO) {
        generatedApi.collections()
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    private fun encodePathSegment(s: String): String =
        java.net.URLEncoder.encode(s, "UTF-8").replace("+", "%20")

    private suspend fun authRequest(builder: Request.Builder, panelRoute: Boolean = false): Request.Builder {
        val snapshot = config.get().let { (baseUrl, auth) -> ClientAuthSnapshot(baseUrl, auth) }
        return builder.withAxonAuth(snapshot, panelRoute)
    }

    private suspend inline fun <reified B, reified R> put(path: String, body: B): Result<R> {
        val bodyBytes = json.encodeToString(body).toRequestBody(JSON_MEDIA_TYPE)
        val builder = runCatching {
            authRequest(
                Request.Builder().url("${baseUrl()}$path").put(bodyBytes),
                panelRoute = path.startsWith("/api/panel/"),
            )
        }.getOrElse { return Result.failure(it) }
        return execute(http, builder)
    }

    private suspend inline fun <reified R> get(path: String): Result<R> {
        val builder = runCatching {
            authRequest(
                Request.Builder().url("${baseUrl()}$path").get(),
                panelRoute = path.startsWith("/api/panel/"),
            )
        }.getOrElse { return Result.failure(it) }
        return execute(http, builder)
    }

    private suspend inline fun <reified R> delete(path: String): Result<R> {
        val builder = runCatching {
            authRequest(
                Request.Builder().url("${baseUrl()}$path").delete(),
                panelRoute = path.startsWith("/api/panel/"),
            )
        }.getOrElse { return Result.failure(it) }
        return execute(http, builder)
    }

    private suspend fun getText(path: String): Result<String> {
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

    private fun baseUrl(): String = config.get().first

    private fun openApiRoute(method: String, template: String, resolved: String = template): String {
        require(method == "GET" || method == "POST" || method == "PUT" || method == "DELETE")
        require(template.startsWith("/v1/"))
        require(resolved.startsWith("/v1/"))
        return resolved
    }

    private suspend inline fun <reified B, reified R> post(path: String, body: B): Result<R> =
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

    private inline fun <reified R> execute(client: OkHttpClient, builder: Request.Builder): Result<R> {
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

    private fun executeText(client: OkHttpClient, builder: Request.Builder): Result<String> {
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

    /**
     * Parses a single SSE data payload into an [AskStreamEvent].
     *
     * Wire format — each event is a JSON object with a `"type"` discriminator:
     * - `{"type":"meta","phase":"retrieval"}` — a processing-phase indicator
     * - `{"type":"delta","text":"..."}` — an incremental LLM token
     * - `{"type":"done","result":{"answer":"..."}}` — synthesis complete; full answer attached
     * - `{"type":"done","answer":"..."}` — older flat completion shape
     * - `{"type":"error","message":"..."}` — server-side failure during streaming
     *
     * Returns null when the type is unknown or the payload is malformed, so the
     * caller can skip unrecognised events without crashing the stream.
     */
    private fun parseStreamEvent(data: String): AskStreamEvent? = runCatching {
        val obj = json.parseToJsonElement(data).jsonObject
        when (obj["type"]?.jsonPrimitive?.content) {
            "meta"  -> AskStreamEvent.Meta(phase = obj["phase"]?.jsonPrimitive?.content ?: "")
            "delta" -> AskStreamEvent.Delta(text = obj["text"]?.jsonPrimitive?.content ?: "")
            "done"  -> AskStreamEvent.Done(
                answer = obj["answer"]?.jsonPrimitive?.contentOrNull
                    ?: obj["result"]?.jsonObject?.get("answer")?.jsonPrimitive?.contentOrNull
                    ?: ""
            )
            "error" -> AskStreamEvent.Error(message = obj["message"]?.jsonPrimitive?.content ?: "Unknown error")
            else    -> null
        }
    }.getOrNull()
}
