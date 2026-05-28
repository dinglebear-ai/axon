package com.axon.app.data.repository.options

import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.intPreferencesKey

internal object MapFormKeys {
    val LIMIT  = intPreferencesKey("mode_options.map.limit")
    val OFFSET = intPreferencesKey("mode_options.map.offset")
    val ALL: List<Preferences.Key<*>> = listOf(LIMIT, OFFSET)
}
