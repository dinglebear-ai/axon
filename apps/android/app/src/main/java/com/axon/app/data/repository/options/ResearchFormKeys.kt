package com.axon.app.data.repository.options

import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.intPreferencesKey

internal object ResearchFormKeys {
    val LIMIT = intPreferencesKey("mode_options.research.limit")
    val ALL: List<Preferences.Key<*>> = listOf(LIMIT)
}
