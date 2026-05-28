package com.axon.app.data.repository.options

import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey

internal object IngestFormKeys {
    val INCLUDE_SOURCE = booleanPreferencesKey("mode_options.ingest.include_source")
    val COLLECTION     = stringPreferencesKey("mode_options.ingest.collection")
    val ALL: List<Preferences.Key<*>> = listOf(INCLUDE_SOURCE, COLLECTION)
}
