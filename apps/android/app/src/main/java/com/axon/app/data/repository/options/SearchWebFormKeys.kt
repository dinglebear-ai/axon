package com.axon.app.data.repository.options

import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey

internal object SearchWebFormKeys {
    val LIMIT      = intPreferencesKey("mode_options.search.limit")
    val OFFSET     = intPreferencesKey("mode_options.search.offset")
    val TIME_RANGE = stringPreferencesKey("mode_options.search.time_range")
    val ALL: List<Preferences.Key<*>> = listOf(LIMIT, OFFSET, TIME_RANGE)
}
