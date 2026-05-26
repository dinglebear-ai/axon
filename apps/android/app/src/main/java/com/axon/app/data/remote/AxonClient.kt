package com.axon.app.data.remote

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.util.concurrent.TimeUnit

private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()

private val json = Json {
    ignoreUnknownKeys = true
    coerceInputValues = true
}

class AxonClient(
    private var baseUrl: String,
    private var token: String,
) {
    private val http = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .writeTimeout(15, TimeUnit.SECONDS)
        .build()

    // Research synthesis can take up to 2 minutes
    private val httpLong = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(120, TimeUnit.SECONDS)
        .writeTimeout(15, TimeUnit.SECONDS)
        .build()

    fun updateConfig(newBaseUrl: String, newToken: String) {
        baseUrl = newBaseUrl.trimEnd('/')
        token = newToken
    }

    suspend fun healthz(): Boolean = withContext(Dispatchers.IO) {
        runCatching {
            val req = Request.Builder()
                .url("$baseUrl/healthz")
                .get()
                .build()
            http.newCall(req).execute().use { it.isSuccessful }
        }.getOrDefault(false)
    }

    suspend fun ask(request: AskRequest): Result<AskResponse> = withContext(Dispatchers.IO) {
        post("/v1/ask", request)
    }

    suspend fun query(request: QueryRequest): Result<QueryResponse> = withContext(Dispatchers.IO) {
        post("/v1/query", request)
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

    suspend fun crawlStatus(jobId: String): Result<JsonObject> = withContext(Dispatchers.IO) {
        get("/v1/crawl/$jobId")
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    private fun authRequest(builder: Request.Builder): Request.Builder =
        builder.header("Authorization", "Bearer $token")
               .header("x-api-key", token)

    private inline fun <reified B, reified R> post(path: String, body: B): Result<R> =
        postWith(http, path, body)

    private inline fun <reified B, reified R> postWith(client: OkHttpClient, path: String, body: B): Result<R> =
        runCatching {
            val bodyBytes = json.encodeToString(body).toRequestBody(JSON_MEDIA_TYPE)
            val req = authRequest(
                Request.Builder()
                    .url("$baseUrl$path")
                    .post(bodyBytes)
            ).build()
            client.newCall(req).execute().use { resp ->
                if (!resp.isSuccessful) {
                    val msg = resp.body?.string() ?: resp.message
                    error("HTTP ${resp.code}: $msg")
                }
                json.decodeFromString<R>(resp.body!!.string())
            }
        }

    private inline fun <reified R> get(path: String): Result<R> =
        runCatching {
            val req = authRequest(Request.Builder().url("$baseUrl$path").get()).build()
            http.newCall(req).execute().use { resp ->
                if (!resp.isSuccessful) {
                    val msg = resp.body?.string() ?: resp.message
                    error("HTTP ${resp.code}: $msg")
                }
                json.decodeFromString<R>(resp.body!!.string())
            }
        }
}
