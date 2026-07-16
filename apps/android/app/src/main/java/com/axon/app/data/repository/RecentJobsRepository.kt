package com.axon.app.data.repository

import android.content.Context
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/** A persisted record of a job the user submitted from this client. */
@Serializable
data class RecentJob(
    val jobId: String,
    val kind: String,    // "source" | "extract"
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
 * Storage is a single JSON-encoded [List] of [RecentJob] objects stored under
 * one [stringPreferencesKey]. Using a single key avoids the `Set<String>` semantic
 * mismatch — DataStore Set identity is referential but jobId-based dedup requires
 * value equality on [RecentJob.jobId].
 */
class RecentJobsRepository(context: Context) {
    private val ds = context.recentJobsDataStore
    private val key = stringPreferencesKey("entries")
    private val json = Json { ignoreUnknownKeys = true }

    private fun decode(raw: String?): List<RecentJob> =
        raw?.let { runCatching { json.decodeFromString<List<RecentJob>>(it) }.getOrNull() } ?: emptyList()

    /** Latest-first stream of persisted entries; ignores undecodable data. */
    val recent: Flow<List<RecentJob>> = ds.data.map { prefs -> decode(prefs[key]) }

    /**
     * Add (or replace) a job entry. Dedupes by [RecentJob.jobId] and enforces
     * the [MAX_RECENT_JOBS] LRU cap. See R6 in the Phase-2 plan revisions.
     */
    suspend fun add(job: RecentJob) {
        ds.edit { prefs ->
            val current = decode(prefs[key])
            val updated = (listOf(job) + current.filterNot { it.jobId == job.jobId }).take(MAX_RECENT_JOBS)
            prefs[key] = json.encodeToString(updated)
        }
    }

    /** Remove the entry with the given `jobId`, if any. */
    suspend fun forget(jobId: String) {
        ds.edit { prefs ->
            val current = decode(prefs[key])
            prefs[key] = json.encodeToString(current.filterNot { it.jobId == jobId })
        }
    }
}
