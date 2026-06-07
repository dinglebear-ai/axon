package com.axon.app.data.repository.options

import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey

internal object CrawlFormKeys {
    val MAX_PAGES          = intPreferencesKey("mode_options.crawl.max_pages")
    val MAX_DEPTH          = intPreferencesKey("mode_options.crawl.max_depth")
    val RENDER_MODE        = stringPreferencesKey("mode_options.crawl.render_mode")
    val INCLUDE_SUBDOMAINS = booleanPreferencesKey("mode_options.crawl.include_subdomains")
    val SKIP_EMBED         = booleanPreferencesKey("mode_options.crawl.skip_embed")
    val COLLECTION         = stringPreferencesKey("mode_options.crawl.collection")
    val WAIT               = booleanPreferencesKey("mode_options.crawl.wait")

    val ALL: List<Preferences.Key<*>> = listOf(
        MAX_PAGES, MAX_DEPTH, RENDER_MODE, INCLUDE_SUBDOMAINS,
        SKIP_EMBED, COLLECTION, WAIT,
    )
}
