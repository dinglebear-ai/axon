package com.axon.app.data.repository

import android.content.Context
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringSetPreferencesKey
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/** A persisted record of a job the user submitted from this client. */
@Serializable
data class RecentJob(
    val jobId: String,
    val kind: String,    // "crawl" | "embed" | "extract" | "ingest"
    val target: String,
    val submittedAt: Long,
)

/**
 * LRU cap for persisted job submissions. R6 sets this at 100 — large enough to
 * cover a power-user's session, small enough to keep DataStore reads cheap.
 */
private const val MAX_RECENT_JOBS = 100

/**
 * Persists the `(jobId, kind, target, submittedAt)` tuple of jobs submitted
 * from this client. Survives process death so the Jobs page can show a
 * "Recent submissions" header even if the user just opened the app.
 *
 * Storage is a JSON-encoded [Set] of [RecentJob] strings inside a Preferences
 * DataStore. [add] dedupes by `jobId` (R6 lock) — re-submitting the same job
 * with a fresh `submittedAt` updates the entry in place rather than producing
 * two distinct Set members (which would happen with a naive `current.add(...)`
 * because JSON-encoded strings with different timestamps are different members).
 */
class RecentJobsRepository(context: Context) {
    private val ds = context.recentJobsDataStore
    private val key = stringSetPreferencesKey("entries")
    private val json = Json { ignoreUnknownKeys = true }

    /** Latest-first stream of persisted entries; ignores undecodable rows. */
    val recent: Flow<List<RecentJob>> = ds.data.map { prefs ->
        (prefs[key] ?: emptySet())
            .mapNotNull { runCatching { json.decodeFromString<RecentJob>(it) }.getOrNull() }
            .sortedByDescending { it.submittedAt }
    }

    /**
     * Add (or replace) a job entry. Dedupes by [RecentJob.jobId] and enforces
     * the [MAX_RECENT_JOBS] LRU cap. See R6 in the Phase-2 plan revisions.
     */
    suspend fun add(job: RecentJob) {
        ds.edit { prefs ->
            // Decode current entries, drop any with the same jobId, prepend the new entry,
            // trim to the LRU cap, re-encode.
            val current = (prefs[key] ?: emptySet())
                .mapNotNull { runCatching { json.decodeFromString<RecentJob>(it) }.getOrNull() }
                .filterNot { it.jobId == job.jobId }
            val updated = (listOf(job) + current).take(MAX_RECENT_JOBS)
            prefs[key] = updated.map { json.encodeToString(it) }.toSet()
        }
    }

    /** Remove the entry with the given `jobId`, if any. */
    suspend fun forget(jobId: String) {
        ds.edit { prefs ->
            val current = prefs[key] ?: return@edit
            prefs[key] = current.filterNot {
                runCatching { json.decodeFromString<RecentJob>(it).jobId == jobId }.getOrDefault(false)
            }.toSet()
        }
    }
}
