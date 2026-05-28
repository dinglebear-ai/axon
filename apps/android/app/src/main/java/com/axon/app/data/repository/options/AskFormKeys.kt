package com.axon.app.data.repository.options

import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey

internal object AskFormKeys {
    val CHUNK_LIMIT       = intPreferencesKey("mode_options.ask.chunk_limit")
    val FULL_DOCS         = intPreferencesKey("mode_options.ask.full_docs")
    val MAX_CONTEXT_CHARS = intPreferencesKey("mode_options.ask.max_context_chars")
    val HYBRID_CANDIDATES = intPreferencesKey("mode_options.ask.hybrid_candidates")
    val DIAGNOSTICS       = booleanPreferencesKey("mode_options.ask.diagnostics")
    val EXPLAIN           = booleanPreferencesKey("mode_options.ask.explain")
    val COLLECTION        = stringPreferencesKey("mode_options.ask.collection")

    val ALL: List<Preferences.Key<*>> =
        listOf(CHUNK_LIMIT, FULL_DOCS, MAX_CONTEXT_CHARS, HYBRID_CANDIDATES, DIAGNOSTICS, EXPLAIN, COLLECTION)
}
