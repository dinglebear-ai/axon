package com.axon.app.data.repository.options

import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey

internal object SourceFormKeys {
    val EMBED = booleanPreferencesKey("mode_options.source.embed")
    val COLLECTION = stringPreferencesKey("mode_options.source.collection")
    val ALL: List<Preferences.Key<*>> = listOf(EMBED, COLLECTION)
}
