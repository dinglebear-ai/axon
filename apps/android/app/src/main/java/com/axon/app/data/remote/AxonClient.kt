package com.axon.app.data.remote

import android.util.Log
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.job
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import com.axon.app.data.remote.models.AcceptedJob
import com.axon.app.data.remote.models.CancelResponse
import com.axon.app.data.remote.models.DoctorResponse
import com.axon.app.data.remote.models.DomainsResponse
import com.axon.app.data.remote.models.IngestRequest
import com.axon.app.data.remote.models.SearchWebRequest
import com.axon.app.data.remote.models.SearchWebResponse
import com.axon.app.data.remote.models.ServiceJob
import com.axon.app.data.remote.models.StatusSummary
import com.axon.app.data.remote.models.SuggestRequest
import com.axon.app.data.remote.models.SuggestResponse
import com.axon.app.data.remote.models.SummarizeRequest
import com.axon.app.data.remote.models.SummarizeResponse
import okhttp3.ConnectionPool
import okhttp3.Dispatcher
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

// ── Timeout constants ─────────────────────────────────────────────────────────

private const val CONNECT_TIMEOUT_SECONDS = 10L
private const val READ_TIMEOUT_SECONDS = 60L

/** Synthesis endpoints (research) can take up to 5 min — matches AXON_LLM_COMPLETION_TIMEOUT_SECS. */
private const val LONG_READ_TIMEOUT_SECONDS = 300L

/**
 * SSE stream read timeout. Must be long enough to span the full LLM generation window.
 * OkHttp's read timeout fires when no *bytes* arrive for this duration — a slow token
 * stream resets it on each chunk, so this is effectively an idle-stream timeout.
 */
private const val STREAM_READ_TIMEOUT_SECONDS = 300L

private const val WRITE_TIMEOUT_SECONDS = 15L

// ─────────────────────────────────────────────────────────────────────────────

private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()

private val json = Json {
    ignoreUnknownKeys = true
    coerceInputValues = true
}

private const val TAG = "AxonClient"

class AxonClient(
    baseUrl: String,
    token: String,
) {
    // Thread-safe config: both baseUrl and token updated atomically together.
    private val config = AtomicReference(baseUrl.trimEnd('/') to token)

    // R7: share a single ConnectionPool + Dispatcher across http/httpLong/httpStream
    // so concurrent fan-out (e.g. polling multiple job kinds) doesn't starve on
    // OkHttp's default `maxRequestsPerHost = 5`.
    private val sharedPool = ConnectionPool(
        maxIdleConnections = 16,
        keepAliveDuration = 5,
        TimeUnit.MINUTES,
    )
    private val sharedDispatcher = Dispatcher().apply { maxRequestsPerHost = 16 }

    private val http = OkHttpClient.Builder()
        .connectionPool(sharedPool)
        .dispatcher(sharedDispatcher)
        .connectTimeout(CONNECT_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .readTimeout(READ_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .writeTimeout(WRITE_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .build()

    // Research synthesis can take up to 5 minutes — built from the shared client to reuse the
    // connection pool and dispatcher.
    private val httpLong = http.newBuilder()
        .readTimeout(LONG_READ_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .build()

    /**
     * Dedicated OkHttp client for SSE streaming. Uses a longer read timeout than [httpLong]
     * because OkHttp's read timeout is an *idle* timeout — it fires when no bytes arrive for
     * the configured duration, not after an absolute wall-clock budget. A slow LLM emitting
     * tokens occasionally keeps the timeout rolling, so we give it the full synthesis window
     * without sharing the connection-timeout semantics of [httpLong].
     */
    private val httpStream = http.newBuilder()
        .readTimeout(STREAM_READ_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .build()

    fun updateConfig(newBaseUrl: String, newToken: String) {
        config.set(newBaseUrl.trimEnd('/') to newToken)
    }

    fun hasToken(): Boolean = config.get().second.isNotBlank()

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
                    error("HTTP ${resp.code}: ${resp.body?.string()?.take(200) ?: resp.message}")
                }
            }
        }.onFailure { if (it is CancellationException) throw it }
    }

    suspend fun ask(request: AskRequest): Result<AskResponse> = withContext(Dispatchers.IO) {
        post("/v1/ask", request)
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
        val bodyBytes = json.encodeToString(request).toRequestBody(JSON_MEDIA_TYPE)
        // Capture atomically once — avoids a TOCTOU race if updateConfig() is called mid-stream.
        val (baseUrl, token) = config.get()
        val req = Request.Builder()
            .url("$baseUrl/v1/ask/stream")
            .post(bodyBytes)
            .header("Authorization", "Bearer $token")
            .header("x-api-key", token)
            .build()

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
                val errBody = resp.body?.string()?.take(200) ?: resp.message
                Log.w(TAG, "askStream: HTTP ${resp.code} $errBody")
                emit(AskStreamEvent.Error("HTTP ${resp.code}: $errBody"))
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
    }.flowOn(Dispatchers.IO)

    suspend fun query(request: QueryRequest): Result<QueryResponse> = withContext(Dispatchers.IO) {
        post("/v1/query", request)
    }

    suspend fun retrieve(request: RetrieveRequest): Result<RetrieveResponse> = withContext(Dispatchers.IO) {
        // Retrieve can return large assembled documents; use the longer-timeout client.
        postWith(httpLong, "/v1/retrieve", request)
    }

    suspend fun sources(request: SourcesRequest = SourcesRequest()): Result<SourcesResponse> =
        withContext(Dispatchers.IO) {
            get("/v1/sources?limit=${request.limit}&offset=${request.offset}")
        }

    suspend fun stats(): Result<StatsResponse> = withContext(Dispatchers.IO) {
        get("/v1/stats")
    }

    suspend fun scrape(request: ScrapeRequest): Result<ScrapeResponse> = withContext(Dispatchers.IO) {
        post("/v1/scrape", request)
    }

    suspend fun map(request: MapRequest): Result<MapResponse> = withContext(Dispatchers.IO) {
        post("/v1/map", request)
    }

    suspend fun research(request: ResearchRequest): Result<ResearchResponse> = withContext(Dispatchers.IO) {
        postWith(httpLong, "/v1/research", request)
    }

    suspend fun crawlSubmit(request: CrawlRequest): Result<CrawlJobResponse> = withContext(Dispatchers.IO) {
        post("/v1/crawl", request)
    }

    suspend fun crawlStatus(jobId: String): Result<CrawlStatusResponse> = withContext(Dispatchers.IO) {
        // The server wraps the job in {"job": {...}}; decode the envelope and unwrap.
        get<CrawlStatusWrapper>("/v1/crawl/$jobId").map { it.job }
    }

    // ── Phase 2 endpoints ──────────────────────────────────────────────────────

    enum class JobKind(val path: String) {
        Crawl("crawl"), Embed("embed"), Extract("extract"), Ingest("ingest")
    }

    /** /v1/summarize — Gemini-backed, can take minutes. Use httpLong. */
    suspend fun summarize(req: SummarizeRequest): Result<SummarizeResponse> = withContext(Dispatchers.IO) {
        postWith(httpLong, "/v1/summarize", req)
    }

    /** /v1/search — Tavily web search; auto-enqueues crawl jobs server-side. */
    suspend fun searchWeb(req: SearchWebRequest): Result<SearchWebResponse> = withContext(Dispatchers.IO) {
        post("/v1/search", req)
    }

    /** POST /v1/ingest — submits an async ingest job. */
    suspend fun ingestStart(req: IngestRequest): Result<AcceptedJob> = withContext(Dispatchers.IO) {
        post("/v1/ingest", req)
    }

    /** GET /v1/{kind}/{id} — job detail. Long-poll-friendly via httpLong. */
    suspend fun getJob(kind: JobKind, id: String): Result<ServiceJob> = withContext(Dispatchers.IO) {
        val builder = authRequest(Request.Builder().url("${baseUrl()}/v1/${kind.path}/$id").get())
        execute(httpLong, builder)
    }

    /** GET /v1/{kind}/list — list jobs of one kind. */
    suspend fun listJobs(kind: JobKind, limit: Int = 100, offset: Int = 0): Result<List<ServiceJob>> = withContext(Dispatchers.IO) {
        get("/v1/${kind.path}/list?limit=$limit&offset=$offset")
    }

    /** POST /v1/{kind}/{id}/cancel. */
    suspend fun cancelJob(kind: JobKind, id: String): Result<CancelResponse> = withContext(Dispatchers.IO) {
        val body = "{}".toRequestBody(JSON_MEDIA_TYPE)
        val builder = authRequest(Request.Builder().url("${baseUrl()}/v1/${kind.path}/$id/cancel").post(body))
        execute(http, builder)
    }

    suspend fun status(): Result<StatusSummary> = withContext(Dispatchers.IO) { get("/v1/status") }

    suspend fun doctor(): Result<DoctorResponse> = withContext(Dispatchers.IO) { get("/v1/doctor") }

    suspend fun suggest(focus: String? = null, collection: String? = null): Result<SuggestResponse> =
        withContext(Dispatchers.IO) { post("/v1/suggest", SuggestRequest(focus = focus, collection = collection)) }

    suspend fun domains(limit: Int = 100, offset: Int = 0): Result<DomainsResponse> =
        withContext(Dispatchers.IO) { get("/v1/domains?limit=$limit&offset=$offset") }

    // ── Helpers ───────────────────────────────────────────────────────────────

    private fun authRequest(builder: Request.Builder): Request.Builder {
        val (_, token) = config.get()
        return builder
            .header("Authorization", "Bearer $token")
            .header("x-api-key", token)
    }

    private fun baseUrl(): String = config.get().first

    private inline fun <reified B, reified R> post(path: String, body: B): Result<R> =
        postWith(http, path, body)

    private inline fun <reified B, reified R> postWith(client: OkHttpClient, path: String, body: B): Result<R> {
        val bodyBytes = json.encodeToString(body).toRequestBody(JSON_MEDIA_TYPE)
        val builder = authRequest(Request.Builder().url("${baseUrl()}$path").post(bodyBytes))
        return execute(client, builder)
    }

    private inline fun <reified R> get(path: String): Result<R> {
        val builder = authRequest(Request.Builder().url("${baseUrl()}$path").get())
        return execute(http, builder)
    }

    private inline fun <reified R> execute(client: OkHttpClient, builder: Request.Builder): Result<R> {
        val built = builder.build()
        return runCatching {
            client.newCall(built).execute().use { resp ->
                if (!resp.isSuccessful) {
                    error("HTTP ${resp.code}: ${resp.body?.string()?.take(200) ?: resp.message}")
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

    /**
     * Parses a single SSE data payload into an [AskStreamEvent].
     *
     * Wire format — each event is a JSON object with a `"type"` discriminator:
     * - `{"type":"meta","phase":"retrieval"}` — a processing-phase indicator
     * - `{"type":"delta","text":"..."}` — an incremental LLM token
     * - `{"type":"done","answer":"..."}` — synthesis complete; full answer attached
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
            "done"  -> AskStreamEvent.Done(answer = obj["answer"]?.jsonPrimitive?.content ?: "")
            "error" -> AskStreamEvent.Error(message = obj["message"]?.jsonPrimitive?.content ?: "Unknown error")
            else    -> null
        }
    }.getOrNull()
}
