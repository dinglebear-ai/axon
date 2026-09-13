package com.axon.app.core.api

import android.util.Log
import com.axon.app.core.api.models.JobStreamEventDto
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.channels.trySendBlocking
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.flowOn
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
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
fun AxonClient.askStream(request: AskRequest): Flow<AskStreamEvent> =
    streamCompletion(openApiRoute("POST", "/v1/ask/stream"), request).flowOn(Dispatchers.IO)

fun AxonClient.chatStream(request: ChatRequest): Flow<AskStreamEvent> =
    streamCompletion(openApiRoute("POST", "/v1/chat/stream"), request).flowOn(Dispatchers.IO)

private inline fun <reified T> AxonClient.streamCompletion(
    path: String,
    request: T,
): Flow<AskStreamEvent> =
    callbackFlow {
        val bodyBytes = json.encodeToString(request).toRequestBody(JSON_MEDIA_TYPE)
        // Capture atomically once — avoids a TOCTOU race if updateConfig() is called mid-stream.
        val requestBuilder =
            runCatching {
                authRequest(
                    Request
                        .Builder()
                        .url("${baseUrl()}$path")
                        .post(bodyBytes),
                )
            }.getOrElse {
                trySend(AskStreamEvent.Error(it.message ?: "No Axon authentication configured"))
                close()
                return@callbackFlow
            }
        val req = requestBuilder.build()

        val call = httpStream.newCall(req)
        call.enqueue(
            object : okhttp3.Callback {
                override fun onFailure(
                    call: okhttp3.Call,
                    error: java.io.IOException,
                ) {
                    if (!call.isCanceled()) trySend(AskStreamEvent.Error(error.message ?: "Stream connect failed"))
                    close()
                }

                override fun onResponse(
                    call: okhttp3.Call,
                    resp: okhttp3.Response,
                ) {
                    resp.use {
                        try {
                            if (!resp.isSuccessful) {
                                val rawBody = resp.body?.string()
                                val humanError = httpErrorMessage(resp.code, rawBody, resp.message)
                                Log.w(TAG, "askStream: $humanError")
                                trySendBlocking(AskStreamEvent.Error(humanError))
                                return
                            }
                            val reader = resp.body?.byteStream()?.bufferedReader()
                            if (reader == null) {
                                trySendBlocking(AskStreamEvent.Error("Empty response body"))
                                return
                            }
                            try {
                                var line: String?
                                while (reader.readLine().also { line = it } != null) {
                                    val l = line ?: break
                                    if (!l.startsWith("data: ")) continue
                                    val data = l.removePrefix("data: ").trim()
                                    if (data.isEmpty()) continue
                                    val event = parseStreamEvent(data) ?: continue
                                    if (trySendBlocking(event).isFailure) break
                                    if (event is AskStreamEvent.Done || event is AskStreamEvent.Error) break
                                }
                            } catch (t: Throwable) {
                                // Socket closed mid-stream (cancel(), timeout, network drop). Surface as
                                // a clean Error so callers can distinguish from a normal Done.
                                if (!call.isCanceled()) trySendBlocking(AskStreamEvent.Error(t.message ?: "Stream interrupted"))
                            } finally {
                                runCatching { reader.close() }
                            }
                        } finally { close() }
                    }
                }
            },
        )
        awaitClose {
            call.cancel()
        }
    }

/**
 * Streams unified job events via SSE from GET /v1/jobs/{id}/stream
 * (android-contract.md `AxonApiClient.streamJobEvents`).
 */
fun AxonClient.streamJobEvents(jobId: String): Flow<JobStreamEventDto> =
    callbackFlow {
        val path = openApiRoute("GET", "/v1/jobs/{id}/stream", "/v1/jobs/${encodePathSegment(jobId)}/stream")
        val requestBuilder =
            runCatching {
                authRequest(Request.Builder().url("${baseUrl()}$path").get())
            }.getOrElse {
                Log.w(TAG, "streamJobEvents: no Axon authentication configured", it)
                close()
                return@callbackFlow
            }
        val call = httpStream.newCall(requestBuilder.build())
        call.enqueue(
            object : okhttp3.Callback {
                override fun onFailure(
                    call: okhttp3.Call,
                    error: java.io.IOException,
                ) {
                    if (!call.isCanceled()) Log.w(TAG, "streamJobEvents: connect failed", error)
                    close()
                }

                override fun onResponse(
                    call: okhttp3.Call,
                    resp: okhttp3.Response,
                ) {
                    resp.use {
                        try {
                            if (!resp.isSuccessful) return
                            val reader = resp.body?.byteStream()?.bufferedReader() ?: return
                            reader.useLines { lines ->
                                for (line in lines) {
                                    if (!line.startsWith("data: ")) continue
                                    val data = line.removePrefix("data: ").trim()
                                    val event = runCatching { json.decodeFromString<JobStreamEventDto>(data) }.getOrNull() ?: continue
                                    if (trySendBlocking(event).isFailure) break
                                    if (event.kind == "final" || event.kind == "error") break
                                }
                            }
                        } catch (t: Throwable) {
                            if (!call.isCanceled()) Log.w(TAG, "streamJobEvents: read failed mid-stream", t)
                        } finally {
                            close()
                        }
                    }
                }
            },
        )
        awaitClose { call.cancel() }
    }.flowOn(Dispatchers.IO)

/**
 * Parses the unified [JobStreamEventDto] envelope used by REST SSE and MCP streaming
 * into the smaller Ask/Chat UI event model.
 *
 * The server contract is discriminated by `kind`; kind-specific fields live in
 * `data`, while structured failures live in `error`.
 */
private fun parseStreamEvent(data: String): AskStreamEvent? =
    runCatching {
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

            "token" -> {
                AskStreamEvent.Delta(
                    text = payload?.get("text")?.jsonPrimitive?.contentOrNull ?: "",
                )
            }

            "final" -> {
                AskStreamEvent.Done(
                    answer =
                        payload?.get("answer")?.jsonPrimitive?.contentOrNull
                            ?: payload?.get("reply")?.jsonPrimitive?.contentOrNull
                            ?: "",
                )
            }

            "error" -> {
                AskStreamEvent.Error(
                    message =
                        envelope.error
                            ?.jsonObject
                            ?.get("message")
                            ?.jsonPrimitive
                            ?.contentOrNull
                            ?: payload?.get("message")?.jsonPrimitive?.contentOrNull
                            ?: "Unknown error",
                )
            }

            // Citation, artifact, and warning events are not represented by the
            // current AskStreamEvent UI model yet. Ignore them without aborting the
            // stream; final/error will still terminate it below.
            else -> {
                null
            }
        }
    }.getOrNull()
