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
 * Parses the unified [JobStreamEventDto] envelope used by REST SSE and MCP streaming
 * into the smaller Ask/Chat UI event model.
 *
 * The server contract is discriminated by `kind`; kind-specific fields live in
 * `data`, while structured failures live in `error`.
 */
private fun parseStreamEvent(data: String): AskStreamEvent? = runCatching {
    val envelope = json.decodeFromString<JobStreamEventDto>(data)
    val payload = envelope.data?.jsonObject
    when (envelope.kind) {
        "progress" -> {
            // SourceProgressEvent.message carries the user-facing phase label
            // (for example "retrieving" / "chatting"). Fall back to the enum
            // phase when a producer omits the message.
            val phase =
                payload?.get("message")?.jsonPrimitive?.contentOrNull
                    ?: payload?.get("phase")?.jsonPrimitive?.contentOrNull
                    ?: ""
            AskStreamEvent.Meta(phase = phase)
        }

        "token" -> AskStreamEvent.Delta(
            text = payload?.get("text")?.jsonPrimitive?.contentOrNull ?: "",
        )

        "final" -> AskStreamEvent.Done(
            answer =
                payload?.get("answer")?.jsonPrimitive?.contentOrNull
                    ?: payload?.get("reply")?.jsonPrimitive?.contentOrNull
                    ?: "",
        )

        "error" -> AskStreamEvent.Error(
            message =
                envelope.error?.jsonObject?.get("message")?.jsonPrimitive?.contentOrNull
                    ?: payload?.get("message")?.jsonPrimitive?.contentOrNull
                    ?: "Unknown error",
        )

        // Citation, artifact, and warning events are not represented by the
        // current AskStreamEvent UI model yet. Ignore them without aborting the
        // stream; final/error will still terminate it below.
        else -> null
    }
}.getOrNull()
