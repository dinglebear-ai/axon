package com.axon.app.data.repository.options

import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey

internal object ScrapeFormKeys {
    val RENDER_MODE = stringPreferencesKey("mode_options.scrape.render_mode")
    val FORMAT      = stringPreferencesKey("mode_options.scrape.format")
    val EMBED       = booleanPreferencesKey("mode_options.scrape.embed")
    val COLLECTION  = stringPreferencesKey("mode_options.scrape.collection")
    val ALL: List<Preferences.Key<*>> = listOf(RENDER_MODE, FORMAT, EMBED, COLLECTION)
}
