package com.axon.app.ui.ingest

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.RecentJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

private const val TAG = "IngestViewModel"

/**
 * Source types the user can ingest from. Each has a `wire` value (server enum) and an
 * optional `targetHostHint` (the canonical host the target URL should resolve to).
 *
 * See R13: client-side host validation is a *defense-in-depth* hint — the server still
 * authoritatively validates the target. We use `URL.host` equality (not substring match)
 * so lookalike hosts like `github.com.attacker.com` are rejected, while non-URL forms
 * (`git@github.com:owner/repo`) flow through unchecked for the server to handle.
 */
enum class IngestSource(val wire: String, private val targetHostHints: Set<String>) {
    Github("github", setOf("github.com")),
    Gitlab("gitlab", setOf("gitlab.com")),
    Gitea("gitea", null),
    Git("git", null),
    Reddit("reddit", setOf("reddit.com")),
    Youtube("youtube", setOf("youtube.com", "youtu.be"));

    constructor(wire: String, targetHostHint: String?) : this(
        wire = wire,
        targetHostHints = targetHostHint?.let { setOf(it) }.orEmpty(),
    )

    val targetHostHint: String? get() = targetHostHints.firstOrNull()

    /**
     * Returns null when the target is acceptable, or a human-readable rejection reason.
     *
     * R13 rules:
     * - blank → reject
     * - no host hint → accept (server validates)
     * - parses as URL → accept iff `host == hint || host.endsWith(".$hint")`, case-insensitive
     * - non-URL form (e.g. `git@host:owner/repo`) → accept (let the server validate)
     */
    fun validate(target: String): String? {
        if (target.isBlank()) return "Target is required"
        val hints = targetHostHints.takeIf { it.isNotEmpty() } ?: return null
        // Non-URL forms (git@host:owner/repo, ssh URIs, etc.) return null from
        // UrlValidator and defer to server-side validation.
        val host = com.axon.app.data.util.UrlValidator.hostOrNull(target) ?: return null
        return if (hints.any { hostMatchesHint(host, it) }) null
        else "Expected target host to be ${hints.joinToString(" or ")}"
    }

    fun matchesHost(host: String): Boolean =
        targetHostHints.any { hostMatchesHint(host.lowercase(), it) }
}

private fun hostMatchesHint(host: String, hint: String): Boolean {
    val lcHint = hint.lowercase()
    return host == lcHint || host.endsWith(".$lcHint")
}

/** Sealed state machine for the Ingest screen. Multi-stage submit flow doesn't map cleanly onto [com.axon.app.ui.common.Resource]. */
sealed interface IngestUi {
    data object Idle : IngestUi
    data object Submitting : IngestUi
    data class Submitted(val jobId: String, val source: IngestSource, val target: String) : IngestUi
    data class Status(val job: JobUi) : IngestUi
    data class Error(val message: String) : IngestUi
}

/**
 * Drives the Ingest mode screen. Submits an async ingest job via `/v1/ingest`, persists
 * the returned `jobId` to [com.axon.app.data.repository.RecentJobsRepository], and offers
 * one-shot status / cancel actions on the resulting Submitted card.
 */
class IngestViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container
    private val _uiState = MutableStateFlow<IngestUi>(IngestUi.Idle)
    val uiState: StateFlow<IngestUi> = _uiState.asStateFlow()

    fun submit(source: IngestSource, target: String) {
        source.validate(target)?.let { msg ->
            _uiState.value = IngestUi.Error(msg)
            return
        }
        viewModelScope.launch {
            _uiState.value = IngestUi.Submitting
            container.axonRepository.ingestStart(source.wire, target).fold(
                onSuccess = { jobId ->
                    runCatching {
                        container.recentJobs.add(
                            RecentJob(
                                jobId = jobId,
                                kind = "ingest",
                                target = target,
                                submittedAt = System.currentTimeMillis(),
                            ),
                        )
                    }.onFailure { Log.w(TAG, "recentJobs.add failed for ingest job $jobId", it) }
                    _uiState.value = IngestUi.Submitted(jobId, source, target)
                },
                onFailure = { _uiState.value = IngestUi.Error(it.message ?: "Error") },
            )
        }
    }

    fun checkStatus(jobId: String) {
        viewModelScope.launch {
            container.axonRepository.getJob(AxonClient.JobKind.Ingest, jobId).fold(
                onSuccess = { _uiState.value = IngestUi.Status(it) },
                onFailure = { _uiState.value = IngestUi.Error(it.message ?: "Error") },
            )
        }
    }

    fun cancel(jobId: String) {
        viewModelScope.launch {
            container.axonRepository.cancelJob(AxonClient.JobKind.Ingest, jobId).fold(
                onSuccess = { canceled ->
                    if (!canceled) {
                        // Server acknowledged the request but reported the job was no longer
                        // cancelable (already finished, already cancelled, unknown). Refresh
                        // the status card so the user sees the real terminal state.
                        Log.w(TAG, "cancelJob ingest/$jobId returned canceled=false")
                    }
                    checkStatus(jobId)
                },
                onFailure = {
                    Log.w(TAG, "cancelJob ingest/$jobId failed", it)
                    // Surface the failure directly — don't mask with a stale status fetch.
                    _uiState.value = IngestUi.Error("Cancel failed: ${it.message ?: "error"}")
                },
            )
        }
    }

    fun reset() { _uiState.value = IngestUi.Idle }
}
