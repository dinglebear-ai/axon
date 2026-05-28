package com.axon.app.data.repository.options

import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey

internal object QueryFormKeys {
    val LIMIT      = intPreferencesKey("mode_options.query.limit")
    val COLLECTION = stringPreferencesKey("mode_options.query.collection")
    val ALL: List<Preferences.Key<*>> = listOf(LIMIT, COLLECTION)
}
