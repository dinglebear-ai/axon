package com.axon.app.core.api

import android.util.Log
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.job
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import com.axon.app.core.api.models.JobStreamEventDto
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

// ── SSE streaming ────────────────────────────────────────────────────────
// Extension functions (not class members) so AxonClient.kt stays under the
// repo's monolith line cap. All use the dedicated [AxonClient.httpStream]
// client so the SSE idle timeout does not interfere with regular request
// timeouts on the normal client. Public call sites are unaffected.

/**
 * Streams the ask response via SSE from POST /v1/ask/stream.
 * Emits [AskStreamEvent.Meta] for phase indicators, [AskStreamEvent.Delta] for each LLM token,
 * [AskStreamEvent.Done] when synthesis completes, and [AskStreamEvent.Error] on failure.
 */
fun AxonClient.askStream(request: AskRequest): Flow<AskStreamEvent> = flow {
    emitAll(streamCompletion(openApiRoute("POST", "/v1/ask/stream"), request))
}.flowOn(Dispatchers.IO)

fun AxonClient.chatStream(request: ChatRequest): Flow<AskStreamEvent> = flow {
    emitAll(streamCompletion(openApiRoute("POST", "/v1/chat/stream"), request))
}.flowOn(Dispatchers.IO)

private inline fun <reified T> AxonClient.streamCompletion(path: String, request: T): Flow<AskStreamEvent> = flow {
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

/**
 * Streams unified job events via SSE from GET /v1/jobs/{id}/stream
 * (android-contract.md `AxonApiClient.streamJobEvents`).
 */
fun AxonClient.streamJobEvents(jobId: String): Flow<JobStreamEventDto> = flow {
    val path = openApiRoute(
        "GET",
        "/v1/jobs/{id}/stream",
        "/v1/jobs/${encodePathSegment(jobId)}/stream",
    )
    val requestBuilder = runCatching {
        authRequest(Request.Builder().url("${baseUrl()}$path").get())
    }.getOrElse {
        Log.w(TAG, "streamJobEvents: no Axon authentication configured", it)
        return@flow
    }
    val req = requestBuilder.build()
    val call = httpStream.newCall(req)
    val cancelHandle = currentCoroutineContext().job.invokeOnCompletion {
        runCatching { call.cancel() }
    }
    val resp = try {
        call.execute()
    } catch (t: Throwable) {
        cancelHandle.dispose()
        if (t is CancellationException) throw t
        Log.w(TAG, "streamJobEvents: connect failed", t)
        return@flow
    }
    try {
        if (!resp.isSuccessful) {
            Log.w(TAG, "streamJobEvents: ${httpErrorMessage(resp.code, resp.body?.string(), resp.message)}")
            return@flow
        }
        val reader = resp.body?.byteStream()?.bufferedReader() ?: return@flow
        try {
            var line: String?
            while (reader.readLine().also { line = it } != null) {
                val l = line ?: break
                if (!l.startsWith("data: ")) continue
                val data = l.removePrefix("data: ").trim()
                if (data.isEmpty()) continue
                val event = runCatching { json.decodeFromString<JobStreamEventDto>(data) }.getOrNull() ?: continue
                emit(event)
                if (event.kind == "final" || event.kind == "error") break
            }
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            Log.w(TAG, "streamJobEvents: read failed mid-stream", t)
        } finally {
            runCatching { reader.close() }
        }
    } finally {
        runCatching { resp.close() }
        cancelHandle.dispose()
    }
}.flowOn(Dispatchers.IO)

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
